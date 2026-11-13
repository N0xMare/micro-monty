//! Repository tasks, run as `cargo xtask <task>`.
//!
//! - `size`: builds the facade's `hello` example as shipped (build-std,
//!   immediate abort, no unwind tables; `wasm-opt -Oz` on wasm) for
//!   `wasm32-unknown-unknown`, the primary budget, and natively, the
//!   secondary one; fails when either exceeds its budget (docs/spec.md §6).
//!   The native budget applies on x86-64 Linux; other hosts report the size.
//! - `conformance`: scores Monty's test corpus on an engine (CPython 3.14 or
//!   micro-monty's `mm`) and writes or checks `conformance/results/`
//!   (docs/spec.md §13).
//! - `bench`: times the §12 benchmark programs per engine and writes
//!   `bench/results/` (docs/spec.md §12).
//! - `durability`: every passing case parked, snapshotted and continued in
//!   a new `mm` process at every park; output must match an unparked run
//!   (docs/spec.md §13).
//! - `parse-diff`: the own parser against the phase-1 ruff front end on
//!   every corpus file (`mm-parse`'s `ruff-oracle` feature, never in default
//!   builds). Recorded difference: ruff accepts a null byte in a comment or
//!   string, which CPython and mm refuse
//!   (`null_bytes_are_refused_where_the_oracle_differs`).
//! - `wasm`: shipped builds of the facade for `wasm32-wasip1` and
//!   `wasm32-unknown-unknown` (`wasm-opt -Oz` included), their sizes, and a
//!   snapshot round trip under wasmtime when it is installed.
//! - `wasm-wall`: wasm wall time of the bench programs against a base
//!   revision, on wasmtime and V8.
//! - `wasm-ir`: wasmtime's guest instructions on the bench programs
//!   against a base revision (callgrind; deterministic).
//! - `gc-stress`: own cases, the corpus and durability on the own cases
//!   with `MM_GC_STRESS=1` (a collection at every safepoint).
//! - `handoffs`: the native-to-Python hand-off sites (`need_py()` calls)
//!   by file and function, written to (`--check`: compared with)
//!   `xtask/handoff-sites.txt`.
//! - `difffuzz`: the differential fuzzer against CPython
//!   (`fuzz/difffuzz/difffuzz.py`).
//! - `check`: every local gate (fmt, clippy, tests, docs, package,
//!   parse-diff, the hand-off site list, size, embedding workload, the golden snapshot images, the
//!   memory-bound probes, the shipped own cases, wasm, conformance `--check`
//!   on both engines and on mm for wasm32-wasip1, the real session replay,
//!   durability (same-target, then native <-> wasm32-wasip1 on the own
//!   cases), bench `--quick`, wasmtime's guest instructions) as stages
//!   run at the same time where they can, then the step-time oracle alone
//!   (`check.rs`). The size ceilings are in `xtask/size-ceilings.txt`.

#![allow(
    clippy::doc_markdown,
    reason = "the docs name CPython and QuickJS in prose"
)]

mod bench;
mod check;
mod conformance;
mod difffuzz;
mod docs;
mod durability;
mod gc_stress;
mod handoffs;
mod package;
mod wasm;
mod wasm_ir;
mod wasm_wall;

use check::Stage;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const USAGE: &str = "usage: cargo xtask <task>

tasks:
  check                      run every local gate: fmt, clippy, test,
                             docs, package, parse-diff, size, embedding workload,
                             the step-time oracle, the golden snapshot images,
                             the memory-bound probes (fuzz/memory_probes), wasm,
                             conformance --check (both engines; micro-monty also
                             on wasm32-wasip1), the real session replay
                             (conformance/codemode/real/gate.py --check), durability,
                             durability --cross-target, bench --quick,
                             wasm-ir (Linux); at the same time where they can, the step-time oracle
                             last and alone; logs in target/check/logs/
      --serial               one stage at a time, output on the terminal
      --release-gate         the full gate: release builds, durability at
                             1:1000, also wasm-host and the codemode check
                             (default: the landing tier, AGENTS.md)
  size                       build `hello` as shipped (wasm-opt -Oz on wasm) for
                             wasm32-unknown-unknown and natively; check both
                             size budgets
  docs                       docs/sdk.md's examples run as micro-monty's doctests;
                             the published crates' rustdoc has no warnings
  package                    `cargo package` the published crates; each carries
                             LICENSE, THIRD_PARTY.md and README.md, and a build
                             from the packages has the workspace's build id
  parse-diff                 clippy and the parity test of mm-parse against
                             the ruff oracle (feature `ruff-oracle`)
  conformance [options]      score conformance/monty on an engine
      --engine <name>        cpython (default) or micro-monty (`$MM`, else
                             target/release/mm built by cargo)
      --filter <substring>   only cases whose file name contains it (writes nothing)
      --check                fail if results differ from conformance/results/
      --timeout <s>          seconds per mm run (default 10)
      --target wasm32-wasip1 micro-monty: mm for wasm32-wasip1 under wasmtime,
                             scored against the native results (writes nothing)
  durability [options]       park every passing case, snapshot it and continue
                             in a new mm process at each park; compare with an
                             unparked run
      --every <n>            fuel units between parks (default: the gate's policy)
      --fine <n:m>           the own cases' and subset's cap (default 1:1000)
      --filter <substring>   only cases whose path contains it
      --timeout <s>          seconds per mm process (default 20)
      --cross-target         own cases only, each snapshot restored on the other
                             target (native <-> wasm32-wasip1 under wasmtime)
      --fuel                 also compare each chain's fuel with the unparked
                             run's; fail on a case not in
                             conformance/durability-fuel-allow.txt (not in check)
  shipped-mm                 build `mm` natively as shipped (the binary the
                             bench Ir rule measures) and print its path
  wasm                       build the facade as shipped (wasm-opt -Oz) for
                             wasm32-wasip1 and wasm32-unknown-unknown, report
                             hello.wasm sizes, run a snapshot round trip under
                             wasmtime
  wasm-host                  the shipped mm-wasm guest (both targets) against
                             crates/mm-wasm-host's release tests, in that
                             separate workspace (not in check)
  wasm-wall [options]        wasm wall time of bench/*.py against a base
                             revision, on wasmtime and V8 (advisory; not in
                             check)
      --base <rev>           the revision to compare with (default origin/main)
      --rounds <n>           rounds, each running both builds (default 11)
      --pool <file>...       judge the rounds of several runs together
                             (target/wasm-wall/rounds/*.tsv, same base and
                             head); builds and times nothing
  wasm-ir [options]          wasmtime guest instructions of bench/*.py against
                             a base revision and an anchor (the last v* tag or
                             accepted rise) under callgrind (in check): fails
                             on a program rising past 1.0%, the geomean past
                             0.5% or startup past 50,000 instructions;
                             MM_WASM_IR_ACCEPT=<reason> passes a rise
      --base <rev>           the revision to compare with (default origin/main)
      --head <rev>           a revision instead of the working tree
      --anchor <rev>         the anchor (default: the last v* tag, or the
                             last Wasm-IR-Accepted commit since it)
      --programs <a,b,..>    only these programs (default all)
  gc-stress                  collect at every safepoint (MM_GC_STRESS=1): own
                             cases must print what they print without, the
                             corpus keep its results, durability on the own
                             cases pass (not in check: about 85 minutes)
  handoffs [--check]         write (--check: compare) xtask/handoff-sites.txt,
                             the need_py() hand-off sites by file and function
  difffuzz [options]         generated programs on mm and CPython 3.14;
                             divergences, minimized, go to
                             fuzz/difffuzz/divergences/
      --seeds <n>            programs (default 100), --start <s> first seed
      --minutes <m>          stop after m minutes; --jobs <j> processes
  bench [options]            time bench/*.py and bench/*.js per engine
      --engines <a,b,..>     cpython,qjs,monty,micro-monty (default); also
                             dquickts,qpy,djs,micro-monty-main
      --reps <n>             repetitions per program (default 10)
      --out-dir <path>       isolated reports, raw samples and manifest
      --timeout-secs <n>     GNU timeout per process (overhead included)
      --quick                1 repetition, check checksums only, write nothing";

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let task = args.next();
    let rest: Vec<String> = args.collect();
    let result = match task.as_deref() {
        Some("check") => check(&rest),
        Some("size") if rest.is_empty() => size(),
        Some("parse-diff") if rest.is_empty() => parse_diff(),
        Some("conformance") => conformance::run(&rest),
        Some("durability") => durability::run(&rest),
        Some("bench") => bench::run(&rest),
        Some("shipped-mm") if rest.is_empty() => shipped_mm(),
        Some("wasm") => wasm::run(&rest),
        Some("wasm-host") if rest.is_empty() => wasm::host(),
        Some("wasm-wall") => wasm_wall::run(&rest),
        Some("wasm-ir") => wasm_ir::run(&rest),
        Some("gc-stress") => gc_stress::run(&rest),
        Some("package") if rest.is_empty() => package::run(),
        Some("docs") if rest.is_empty() => docs::run(),
        Some("difffuzz") => difffuzz::run(&rest),
        Some("handoffs") => handoffs::run(&rest),
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Every local gate, as stages that run at the same time where they can
/// (`check.rs`); `--release-gate` adds `wasm-host` and the codemode check.
///
/// Three things order the stages. Builds in one target directory take
/// turns on cargo's lock, so the debug gates build in `target/check/debug`
/// and the release and shipped builds in `target/`, where xtask and the
/// scripts find them. Stages that write the same file run one after the
/// other: `size` and `wasm` both build `hello` for wasm32-unknown-unknown,
/// `size` and `wasm-host` the guests and their `shipped/` modules, the
/// wasip1 corpus run and the cross-target durability run precompile the
/// same `mm.cwasm`, and the shipped own cases rebuild the debug `mm` that
/// `test`'s own cases run. Durability is the longest stage, and its
/// chains of `mm` processes slow most under load: its release `mm` builds
/// before the other stages in `target/`, and it alone (with that build)
/// runs at normal priority, the rest at `nice` 10. The step-time oracle
/// measures wall time, so it runs last, alone.
#[allow(clippy::too_many_lines, reason = "one flat list of stages")]
fn check(args: &[String]) -> Result<(), String> {
    let (mut serial, mut release_gate) = (false, false);
    for arg in args {
        match arg.as_str() {
            "--serial" => serial = true,
            "--release-gate" => release_gate = true,
            other => return Err(format!("check: unknown option `{other}`\n{USAGE}")),
        }
    }
    let root = workspace_root();
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    // Stages run a copy of this binary, which no stage's build replaces.
    let xtask = root.join("target/check/xtask");
    let exe = env::current_exe().map_err(|e| format!("cannot find xtask's binary: {e}"))?;
    if let Some(dir) = xtask.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    fs::copy(&exe, &xtask).map_err(|e| format!("cannot copy {}: {e}", exe.display()))?;
    let debug = root.join("target/check/debug");
    // The landing tier builds what it only runs (no size or time measured)
    // with thin LTO (`gate`), and its tests at opt 1 with debug assertions
    // (`gate-test`); the release gate builds them as the release profile
    // and `cargo test` do (Cargo.toml).
    let (profile, test_profile) = if release_gate {
        ("release", "test")
    } else {
        ("gate", "gate-test")
    };
    let command = |program: &Path, args: &[&str], dir: Option<&Path>| {
        let mut command = Command::new(program);
        command.args(args).current_dir(&root);
        if let Some(dir) = dir {
            command.env("CARGO_TARGET_DIR", dir);
        }
        command
    };
    let cargo = Path::new(&cargo);
    let task = |args: &[&str]| command(&xtask, args, None);
    // A task on the release `mm` stage `mm` built: no cargo call, which
    // would wait for `target/`'s lock while other stages build there.
    let mm = root.join(format!("target/{profile}/mm"));
    let mm_task = |args: &[&str]| {
        let mut command = task(args);
        command.env("MM", &mm).env("MM_PROFILE", profile);
        command
    };
    let debug_task = |args: &[&str]| command(&xtask, args, Some(&debug));
    let debug_cargo = |args: &[&str]| command(cargo, args, Some(&debug));
    let release_cargo = |args: &[&str]| command(cargo, args, None);
    let host = host_target()?;
    let shipped_mm = root.join(format!("target/{host}/release/mm"));
    let steps = [
        "test",
        "--release",
        "--locked",
        "--package",
        "micro-monty",
        "--features=mod-unicodedata,mod-ast",
        "--test",
        "steps",
    ];
    // The step-time oracle times shipped codegen: `--cfg mm_shipped` turns
    // on the `#[optimize(size)]` the shipped builds carry (docs/spec.md
    // §6), in its own target directory so the flag does not rebuild
    // `target/release`. Its panics keep their messages (unwinding, std as
    // installed): they report the failing rows.
    let steps_dir = root.join("target/check/steps");
    let steps_cargo = |args: &[&str]| {
        let mut command = command(cargo, args, Some(&steps_dir));
        command
            .env("RUSTC_BOOTSTRAP", "1")
            .env("RUSTFLAGS", "--cfg mm_shipped")
            .env_remove("CARGO_ENCODED_RUSTFLAGS");
        command
    };
    let clippy = |package: &[&str]| {
        let mut args = vec!["clippy"];
        args.extend_from_slice(package);
        args.extend(["--all-targets", "--locked", "--", "-D", "warnings"]);
        debug_cargo(&args)
    };
    let mut stages = vec![
        Stage::new("fmt").run(release_cargo(&["fmt", "--all", "--check"])),
        // Release `mm` first in `target/`, for durability, the longest
        // run, which runs at full priority (and then the other stages).
        Stage::new("mm").urgent().run(release_cargo(&[
            "build",
            "--profile",
            profile,
            "--locked",
            "--package",
            "mm-cli",
        ])),
        // The landing tier parks the own cases and the corpus subset at
        // most about 100 times each beyond `5:50`, still at every host
        // call; the release gate, about 1,000 (xtask/src/durability.rs).
        Stage::new("durability")
            .after(&["mm"])
            .urgent()
            .run(mm_task(if release_gate {
                &["durability"]
            } else {
                &["durability", "--fine", "1:100"]
            })),
        Stage::new("conformance").run(task(&["conformance", "--check"])),
        Stage::new("conformance-mm").after(&["mm"]).run(mm_task(&[
            "conformance",
            "--engine",
            "micro-monty",
            "--check",
        ])),
        // Normal priority too: tests with wall bounds (parking.rs's 5 s on
        // an endless iterator's three fuel slices) must not starve behind
        // durability. At `nice` 10 a 0.3 s op took 5.4 to 6.4 s in three
        // release-gate runs of three [M].
        Stage::new("test").urgent().run(debug_cargo(&[
            "test",
            "--workspace",
            "--locked",
            "--profile",
            test_profile,
        ])),
        // Builds the workspace never makes (its features unify): the
        // engine with no module features, the facade with no modules, and
        // the facade opting out of `asyncio` and `json` and into `ast`
        // (tests/features.rs).
        Stage::new("clippy")
            .run(clippy(&["--workspace"]))
            .run(clippy(&["--package", "mm-engine"]))
            .run(clippy(&[
                "--package",
                "micro-monty",
                "--no-default-features",
            ])),
        Stage::new("features").run(debug_cargo(&[
            "test",
            "--package",
            "micro-monty",
            "--no-default-features",
            "--features",
            "mod-ast",
            "--locked",
            "--profile",
            test_profile,
            "--test",
            "features",
        ])),
        Stage::new("docs").run(debug_task(&["docs"])),
        Stage::new("package").run(debug_task(&["package"])),
        Stage::new("parse-diff").run(debug_task(&["parse-diff"])),
        Stage::new("handoffs").run(task(&["handoffs", "--check"])),
        // The memory bound (docs/spec.md §11): fuzz/memory_probes under the
        // counting allocator, Rust heap <= 2.5 x limit + 16 MiB.
        Stage::new("memory-probes").run(release_cargo(&[
            "run",
            "--release",
            "--locked",
            "--manifest-path",
            "fuzz/Cargo.toml",
            "--features",
            "memory-probes",
            "--bin",
            "memory_probes",
        ])),
        Stage::new("size").after(&["mm"]).run(task(&["size"])),
        Stage::new("wasm").after(&["size"]).run(task(&["wasm"])),
        // The corpus on mm for wasm32-wasip1 under wasmtime, scored against
        // the native results.
        Stage::new("conformance-wasip1")
            .after(&["mm"])
            .run(mm_task(&[
                "conformance",
                "--engine",
                "micro-monty",
                "--target",
                "wasm32-wasip1",
                "--check",
            ])),
        Stage::new("durability-cross")
            .after(&["conformance-wasip1"])
            .run(mm_task(&["durability", "--cross-target"])),
        // Every recorded real cell replayed through mm-cell-worker.
        Stage::new("real-replay")
            .after(&["mm"])
            .run(release_cargo(&[
                "build",
                "--profile",
                profile,
                "--locked",
                "--package",
                "mm-cell-worker",
                "--example",
                "replay_worker",
            ]))
            .run(command(
                &python(),
                &[
                    "conformance/codemode/real/gate.py",
                    "--check",
                    "--worker",
                    &format!("target/{profile}/examples/replay_worker"),
                ],
                None,
            )),
        Stage::new("bench-quick").after(&["mm"]).run(mm_task(&[
            "bench",
            "--quick",
            "--engines",
            "cpython,micro-monty",
        ])),
        // The own cases against CPython on `mm` built natively as shipped
        // (`shipped_build`: `-Cpanic=immediate-abort`, `#[optimize(size)]`
        // under `mm_shipped`); the step-time oracle runs on `mm_shipped`
        // codegen too (`steps_cargo`), every other native gate on
        // plain-stable builds.
        Stage::new("shipped-mm").after(&["mm"]).run({
            let mut build = shipped_build(&host);
            build.args(["--package", "mm-cli", "--bin", "mm"]);
            build
        }),
        Stage::new("shipped-own-cases")
            .after(&["shipped-mm", "test"])
            .run({
                let mut test = debug_cargo(&[
                    "test",
                    "--locked",
                    "--profile",
                    test_profile,
                    "--package",
                    "mm-cli",
                    "--test",
                    "conformance_own",
                ]);
                test.env("MM", &shipped_mm);
                test
            }),
        Stage::new("embedding-bench")
            .after(&["mm"])
            .run(release_cargo(&[
                "run",
                "--release",
                "--locked",
                "--package",
                "micro-monty",
                "--example",
                "embedding_bench",
            ])),
        // The golden snapshot images restore and continue (docs/spec.md
        // §9.3): in the facade's own build, as they are blessed, since the
        // workspace's has mm-cli's opt-in modules and so another build id.
        Stage::new("golden").after(&["mm"]).run(release_cargo(&[
            "test",
            "--profile",
            profile,
            "--locked",
            "--package",
            "micro-monty",
            "--test",
            "golden",
        ])),
        Stage::new("steps-build")
            .after(&["mm"])
            .run(steps_cargo(&[&steps[..], &["--no-run"]].concat())),
    ];
    // wasmtime's guest instructions against origin/main, ±0.5% (its own
    // worktree and target directories under target/wasm-ir/).
    if cfg!(target_os = "linux") {
        stages.push(Stage::new("wasm-ir").run(task(&["wasm-ir"])));
    } else {
        println!("check: wasm-ir skipped: valgrind runs on Linux only");
    }
    if release_gate {
        stages.push(
            Stage::new("wasm-host")
                .after(&["size"])
                .run(task(&["wasm-host"])),
        );
        stages.push(Stage::new("codemode").run(command(
            &python(),
            &["conformance/codemode/run.py", "--check"],
            None,
        )));
    }
    // The step-time oracle: its fuel half beside the other stages (fuel is
    // deterministic), its wall-time half in release and alone.
    stages.push(
        Stage::new("steps-fuel")
            .after(&["steps-build"])
            .run(steps_cargo(
                &[&steps[..], &["--", "slice_charges"]].concat(),
            )),
    );
    stages.push(
        Stage::new("steps")
            .after(&["steps-build"])
            .alone()
            .run(steps_cargo(
                &[
                    &steps[..],
                    &["--", "--skip", "slice_charges", "--test-threads=1"],
                ]
                .concat(),
            )),
    );
    check::run(stages, &root, serial)
}

fn run_gate(cargo: &str, args: &[&str]) -> Result<(), String> {
    println!("check: cargo {}", args.join(" "));
    let status = Command::new(cargo)
        .args(args)
        .current_dir(workspace_root())
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("gate failed: cargo {}", args.join(" ")))
    }
}

/// The differential gate: `mm-parse` with its `ruff-oracle` feature (the
/// phase-1 ruff front end) must agree with the own parser on every corpus
/// file, own case, bench program and the prelude. A separate cargo
/// invocation, so ruff never enters the default build.
fn parse_diff() -> Result<(), String> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    run_gate(
        &cargo,
        &[
            "clippy",
            "--package",
            "mm-parse",
            "--features",
            "ruff-oracle",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run_gate(
        &cargo,
        &[
            "test",
            "--package",
            "mm-parse",
            "--features",
            "ruff-oracle",
            "--locked",
            "--test",
            "ruff_parity",
        ],
    )
}

fn size() -> Result<(), String> {
    let ceilings = size_ceilings()?;
    let ceiling = |artifact: &str| {
        ceilings
            .get(artifact)
            .copied()
            .ok_or_else(|| format!("{CEILINGS} has no ceiling for {artifact}"))
    };
    let wasm = shipped_example(SIZE_TARGET_WASM, "hello")?;
    let native_target = host_target()?;
    let native = shipped_example(&native_target, "hello")?;
    let mut over = Vec::new();
    let mut budgets = vec![(
        "wasm32-unknown-unknown hello.wasm",
        &wasm,
        ceiling("hello.wasm")?,
    )];
    if native_target == SIZE_TARGET_NATIVE {
        budgets.push(("native hello", &native, ceiling("hello")?));
    } else {
        println!(
            "size: native hello {} bytes on {native_target} (reported; the native budget applies to {SIZE_TARGET_NATIVE})",
            wasm::file_size(&native)?
        );
    }
    let guest = shipped_guest()?;
    budgets.push((
        "wasm32-unknown-unknown mm_wasm_guest.wasm",
        &guest,
        ceiling("mm_wasm_guest.wasm")?,
    ));
    let bundle = wasm::shipped_bundle_guest(SIZE_TARGET_WASM)?;
    budgets.push((
        "wasm32-unknown-unknown mm_wasm_guest_bundle.wasm",
        &bundle,
        ceiling("mm_wasm_guest_bundle.wasm")?,
    ));
    for (what, path, budget) in budgets {
        let bytes = wasm::file_size(path)?;
        println!("size: {what} {bytes} bytes (budget {budget})");
        if bytes > budget {
            over.push(format!(
                "{what} is {bytes} bytes, over the {budget}-byte budget"
            ));
        }
    }
    if over.is_empty() {
        Ok(())
    } else {
        Err(over.join("; "))
    }
}

/// The size ceilings, one per artifact, and their history.
const CEILINGS: &str = "xtask/size-ceilings.txt";

/// Each artifact's ceiling in [`CEILINGS`]: the sum of its lines (`<artifact>
/// <bytes> <why>`, bytes signed after the first), `#` lines being comments.
fn size_ceilings() -> Result<std::collections::BTreeMap<String, u64>, String> {
    let text = fs::read_to_string(workspace_root().join(CEILINGS))
        .map_err(|error| format!("cannot read {CEILINGS}: {error}"))?;
    let mut sums = std::collections::BTreeMap::<String, i64>::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(artifact), Some(bytes), Some(_why)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err(format!(
                "{CEILINGS}:{}: not `<artifact> <bytes> <why>`",
                n + 1
            ));
        };
        let bytes: i64 = bytes
            .parse()
            .map_err(|_| format!("{CEILINGS}:{}: `{bytes}` is not a number of bytes", n + 1))?;
        *sums.entry(artifact.to_owned()).or_default() += bytes;
    }
    sums.into_iter()
        .map(|(artifact, sum)| {
            u64::try_from(sum)
                .map(|sum| (artifact.clone(), sum))
                .map_err(|_| format!("{CEILINGS}: the ceiling of {artifact} is negative"))
        })
        .collect()
}

/// The target of the primary size budget.
const SIZE_TARGET_WASM: &str = "wasm32-unknown-unknown";

/// The host the secondary (native) budget was measured on. Executable
/// formats and code size differ per target (Mach-O on aarch64 macOS, for
/// one), so other hosts report the native size without a budget.
const SIZE_TARGET_NATIVE: &str = "x86_64-unknown-linux-gnu";

/// Builds the facade's example `name` as shipped for `target` and returns
/// its path; on wasm, the module `wasm-opt -Oz` wrote ([`wasm::optimize`]).
/// The example's own crate is compiled at opt 3, as an embedder's binary
/// is: it belongs to the facade's package, whose release profile is `z`,
/// and fat LTO optimizes the whole program at the root crate's level
/// (Cargo.toml).
pub(crate) fn shipped_example(target: &str, name: &str) -> Result<PathBuf, String> {
    let status = shipped_cargo("rustc", target)
        .args(["--package", "micro-monty", "--example", name])
        .args(["--", "-Copt-level=3"])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(format!(
            "shipped release build of {name} for {target} failed"
        ));
    }
    let release = workspace_root().join(format!("target/{target}/release/examples"));
    if target.starts_with("wasm32") {
        wasm::optimize(&release.join(format!("{name}.wasm")))
    } else {
        Ok(release.join(name))
    }
}

/// Builds the mm-wasm guest (`crates/mm-wasm-guest`) as shipped for
/// `wasm32-unknown-unknown`, without `test-hooks`, and returns the module
/// `wasm-opt -Oz` wrote.
fn shipped_guest() -> Result<PathBuf, String> {
    let target = SIZE_TARGET_WASM;
    let status = shipped_build(target)
        .args(["--package", "mm-wasm-guest"])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(format!(
            "shipped release build of mm-wasm-guest for {target} failed"
        ));
    }
    wasm::optimize(&workspace_root().join(format!("target/{target}/release/mm_wasm_guest.wasm")))
}

/// Builds `mm` natively as shipped, `target/<host>/release/mm`: the binary
/// `bench/callgrind.py` measures by default.
fn shipped_mm() -> Result<(), String> {
    let host = host_target()?;
    let status = shipped_build(&host)
        .args(["--package", "mm-cli", "--bin", "mm"])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(format!("shipped release build of mm for {host} failed"));
    }
    println!(
        "{}",
        workspace_root()
            .join(format!("target/{host}/release/mm"))
            .display()
    );
    Ok(())
}

/// `cargo build --release` for `target` the way shipped binaries are built
/// (docs/spec.md §6): std rebuilt with `-Cpanic=immediate-abort` (no panic
/// messages, formatting or backtrace symbolization) and without unwind
/// tables. Both need unstable flags, enabled on the pinned stable toolchain
/// with `RUSTC_BOOTSTRAP=1`; std's source comes from the `rust-src`
/// component (rust-toolchain.toml). The explicit `--target` keeps the flags
/// off build scripts and proc macros. Tests, clippy and other builds use
/// plain stable.
pub(crate) fn shipped_build(target: &str) -> Command {
    shipped_cargo("build", target)
}

/// [`shipped_build`] with another cargo `subcommand` (`rustc`).
fn shipped_cargo(subcommand: &str, target: &str) -> Command {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut command = Command::new(cargo);
    command
        .env("RUSTC_BOOTSTRAP", "1")
        .env(
            "RUSTFLAGS",
            "-Zunstable-options -Cpanic=immediate-abort -Cforce-unwind-tables=no --cfg mm_shipped",
        )
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args([
            subcommand,
            "--release",
            "--locked",
            "-Zbuild-std=std,panic_abort",
        ])
        .args(["--target", target])
        .current_dir(workspace_root());
    command
}

/// The host target triple, from `rustc -vV`.
fn host_target() -> Result<String, String> {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let output = command_output(Path::new(&rustc), &["-vV"])?;
    output
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .ok_or_else(|| format!("no host line in `{rustc} -vV`"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The CPython 3.14 interpreter: `$PYTHON` if set, else the one `.agents/setup`
/// installs at `~/.local/bin/python3.14`, else `python3.14` on `PATH`.
fn python() -> PathBuf {
    if let Some(python) = env::var_os("PYTHON").filter(|value| !value.is_empty()) {
        return PathBuf::from(python);
    }
    if let Some(home) = env::var_os("HOME") {
        let local = Path::new(&home).join(".local/bin/python3.14");
        if local.is_file() {
            return local;
        }
    }
    PathBuf::from("python3.14")
}

/// The `mm` binary: `$MM` if set, else `target/release/mm`, built first with
/// `cargo build --release --package mm-cli`.
fn micro_monty_binary() -> Result<PathBuf, String> {
    if let Some(mm) = env::var_os("MM").filter(|value| !value.is_empty()) {
        let mm = PathBuf::from(mm);
        return if mm.is_file() {
            Ok(mm)
        } else {
            Err(format!("$MM is {}, which does not exist", mm.display()))
        };
    }
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let status = Command::new(cargo)
        .args(["build", "--release", "--quiet", "--package", "mm-cli"])
        .current_dir(workspace_root())
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err("release build of mm (mm-cli) failed".to_owned());
    }
    Ok(workspace_root().join("target/release/mm"))
}

/// Runs `program args` and returns its trimmed stdout, or why it failed.
fn command_output(program: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", program.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} {} failed: {}",
            program.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// `python --version` as `3.14.7`, failing unless it is CPython 3.14.
fn cpython_oracle_version(python: &Path) -> Result<String, String> {
    let version = command_output(
        python,
        &[
            "-c",
            "import platform; print(platform.python_implementation(), platform.python_version())",
        ],
    )?;
    match version.split_once(' ') {
        Some(("CPython", number)) if number.starts_with("3.14.") => Ok(number.to_owned()),
        _ => Err(format!(
            "{} is {version}, not CPython 3.14; set PYTHON to a CPython 3.14 interpreter",
            python.display()
        )),
    }
}
