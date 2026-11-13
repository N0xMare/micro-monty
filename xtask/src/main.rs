//! Repository tasks, run as `cargo xtask <task>`.
//!
//! - `size`: builds the facade's `hello` example as shipped (build-std,
//!   immediate abort, no unwind tables; `wasm-opt -Oz` on wasm) for
//!   `wasm32-unknown-unknown` and natively, and the mm-wasm guests, and
//!   the same for the base and the release they descend from; fails when
//!   one grows past the base without a reason in a commit message, or past
//!   the release's budget in `xtask/size-budget.txt` (`size.rs`,
//!   docs/spec.md §6). The native size is gated on x86-64 Linux; other
//!   hosts report it.
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
//!   (`check.rs`).

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
mod native_ir;
mod package;
mod shipped;
mod size;
mod v8_ir;
mod wasm;
mod wasm_ir;
mod wasm_wall;

use check::Stage;
pub(crate) use shipped::{
    shipped_artifacts, shipped_build, shipped_example, shipped_guests, shipped_mm,
    shipped_mm_binary, shipped_sizes,
};
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
                             (default: the landing tier, or the release gate
                             when the change touches parking, snapshots, the
                             GC, the wasm crates or a profile; AGENTS.md)
      --landing-tier         the landing tier whatever the change touches
  size [--base <rev>]        build `hello` and the mm-wasm guests as shipped
                             (wasm-opt -Oz on wasm), and the base's (default
                             origin/main) and the release's, in worktrees
                             under target/size/ (cached per commit); a rise
                             past the base needs committed trailers `Size:
                             <artifact> +<bytes> <why>` that sum to cover it, past the
                             release a budget in xtask/size-budget.txt
                             (`check` runs it as --references, --head and
                             --judge, so the base builds beside the head)
  docs                       docs/user-guide.md's examples run as micro-monty's doctests;
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
  durability [options]       run every passing case unparked, parked, parked
                             with snapshots, and parked with each park restored
                             in a new mm process; compare reports, fuel and
                             final-state digests, allowing only the differences
                             conformance/durability-results.txt records
      --every <n>            fuel units between parks (default: the gate's policy)
      --fine <n:m>           the own cases' and subset's cap (default 1:1000)
      --filter <substring>   only cases whose path contains it
      --timeout <s>          seconds per mm process (default 20)
      --cross-target         own cases only, each snapshot restored on the other
                             target (native <-> wasm32-wasip1 under wasmtime)
      --record               rewrite the results file without the entries no
                             run differed in (a whole run; never adds one)
      --report-only          compare reports only (gc-stress)
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
  native-ir [--base <rev>]   bench/*.py's gated callgrind Ir on the shipped
                             native mm ($MM, else built) against the base's
                             (default origin/main; built in a worktree under
                             target/native-ir/, cached per commit): fails on
                             a program rising past 0.5%, unless a commit since
                             the base has a Native-IR-Accepted: line or
                             MM_IR_ACCEPT=<reason> is set
  v8-ir [options]            V8's instructions (node --no-liftoff, generated
                             code, the second of two runs) on bench/*.py in
                             both wasm32-unknown-unknown guests against a base
                             and an anchor, as wasm-ir (in the release gate):
                             fails on a program rising past 0.5% or a guest's
                             geomean past 0.3%; MM_V8_IR_ACCEPT=<reason>
                             passes a rise
      --base <rev>           the revision to compare with (default origin/main)
      --anchor <rev>         the anchor (default: the last v* tag, or the
                             last commit since with a V8-IR-Accepted: line)
      --programs <a,b>       only these programs
      --head-built           the head's guests from target/size/head.tsv
  wasm-ir [options]          wasmtime guest instructions of bench/*.py against
                             a base revision and an anchor (the last v* tag or
                             accepted rise) under callgrind (in check): fails
                             on a program rising past 1.0%, the geomean past
                             0.5% or startup past 50,000 instructions;
                             MM_WASM_IR_ACCEPT=<reason> passes a rise
      --base <rev>           the revision to compare with (default origin/main)
      --head <rev>           a revision instead of the working tree
      --anchor <rev>         the anchor (default: the last v* tag, or the
                             last Wasm-IR-Accepted commit since it; none
                             fails); each program's least count from it
                             to the base is what a rise counts from
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
        Some("size") => size(&rest),
        Some("parse-diff") if rest.is_empty() => parse_diff(),
        Some("conformance") => conformance::run(&rest),
        Some("durability") => durability::run(&rest),
        Some("bench") => bench::run(&rest),
        Some("shipped-mm") if rest.is_empty() => shipped_mm(),
        Some("wasm") => wasm::run(&rest),
        Some("wasm-host") if rest.is_empty() => wasm::host(),
        Some("wasm-wall") => wasm_wall::run(&rest),
        Some("wasm-ir") => wasm_ir::run(&rest),
        Some("native-ir") => native_ir::run(&rest),
        Some("v8-ir") => v8_ir::run(&rest),
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
    let (mut serial, mut release_gate, mut landing_tier) = (false, false, false);
    for arg in args {
        match arg.as_str() {
            "--serial" => serial = true,
            "--release-gate" => release_gate = true,
            "--landing-tier" => landing_tier = true,
            other => return Err(format!("check: unknown option `{other}`\n{USAGE}")),
        }
    }
    let root = workspace_root();
    if !release_gate
        && !landing_tier
        && let Some(why) = release_gate_reason(&root)
    {
        println!("check: the release gate, as the change touches {why} (AGENTS.md)");
        release_gate = true;
    }
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
    // The step-time oracle times the shipped native code: built as
    // `shipped_build` builds (std rebuilt, `-Cpanic=immediate-abort`, no
    // unwind tables, `--cfg mm_shipped` and its `#[optimize(size)]`;
    // docs/spec.md §6), with libtest rebuilt too and each test in its own
    // process (`-Zpanic-abort-tests`), in its own target directory. A
    // failing test aborts without its panic message; its output, which
    // marks each row over the limit `OVER`, is still shown. With the stock
    // std, `repr([S])` read 8.7 ms against 32-36 ms as shipped [M, size
    // review].
    let steps = [
        "test",
        "--release",
        "--locked",
        "-Zbuild-std=std,panic_abort,test",
        "-Zpanic-abort-tests",
        "--target",
        &host,
        "--package",
        "micro-monty",
        "--features=mod-unicodedata,mod-ast",
        "--test",
        "steps",
    ];
    let steps_dir = root.join("target/check/steps");
    let steps_cargo = |args: &[&str]| {
        let mut command = command(cargo, args, Some(&steps_dir));
        command
            .env("RUSTC_BOOTSTRAP", "1")
            .env("RUSTFLAGS", shipped::SHIPPED_RUSTFLAGS)
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
                &["durability", "--fine", durability::LANDING_FINE]
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
        // The size gate in three stages: the base's and the release's
        // builds, each in its own target directory under target/size/,
        // beside the head's.
        Stage::new("size-references").run(task(&["size", "--references"])),
        Stage::new("size-head")
            .after(&["mm"])
            .run(task(&["size", "--head"])),
        Stage::new("size")
            .after(&["size-head", "size-references"])
            .run(task(&["size", "--judge"])),
        Stage::new("wasm")
            .after(&["size-head"])
            .run(task(&["wasm"])),
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
        // The own cases against CPython and the corpus against its results,
        // on `mm` built natively as shipped (`shipped_build`:
        // `-Cpanic=immediate-abort`, `#[optimize(size)]` under
        // `mm_shipped`); the step-time oracle runs on shipped codegen too
        // (`steps_cargo`), every other native gate on plain-stable builds.
        Stage::new("shipped-mm").after(&["mm"]).run({
            let mut build = shipped_build(&host);
            build.args(["--package", "mm-cli", "--bin", "mm"]);
            build
        }),
        Stage::new("shipped-cases")
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
            })
            .run({
                let mut corpus = task(&["conformance", "--engine", "micro-monty", "--check"]);
                corpus.env("MM", &shipped_mm);
                corpus
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
        // The bench programs' gated Ir on the shipped native `mm` against
        // the base's: a rise past 0.5% fails (AGENTS.md's Ir rule).
        stages.push(Stage::new("native-ir").after(&["shipped-mm"]).run({
            let mut ir = task(&["native-ir"]);
            ir.env("MM", &shipped_mm);
            ir
        }));
    } else {
        println!("check: wasm-ir and native-ir skipped: valgrind runs on Linux only");
    }
    if release_gate && cfg!(target_os = "linux") {
        // V8's instructions on the guests the size-head stage built.
        stages.push(
            Stage::new("v8-ir")
                .after(&["size-head"])
                .run(task(&["v8-ir", "--head-built"])),
        );
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

/// Paths (prefixes) whose change makes plain `check` the release gate:
/// parking, snapshots, the GC, the wasm crates and how the wasm guests are
/// built (`v8-ir` gates that).
const RELEASE_GATE_PATHS: &[&str] = &[
    "crates/mm-engine/src/interp.rs",
    "crates/mm-engine/src/snapshot.rs",
    "crates/mm-engine/src/nstep.rs",
    "crates/mm-core/src/gc.rs",
    "crates/mm-core/src/snapshot.rs",
    "crates/micro-monty/src/image.rs",
    "crates/mm-compile/src/encode.rs",
    "crates/mm-wasm-",
    "xtask/src/wasm.rs",
];

/// Why the change since the merge base with `origin/main`, committed or
/// not (new untracked files included), needs the release gate: a path of [`RELEASE_GATE_PATHS`], Cargo.toml's
/// `[profile.*]` tables, an `optimize(size)` mark added or removed, or no
/// merge base to tell.
fn release_gate_reason(root: &Path) -> Option<String> {
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let Some(base) = git(&["merge-base", "origin/main", "HEAD"]) else {
        return Some("an unknown set of files (no merge base with origin/main)".to_owned());
    };
    let base = base.trim();
    let changed = git(&["diff", "--name-only", base]).unwrap_or_default()
        + &git(&["ls-files", "--others", "--exclude-standard"]).unwrap_or_default();
    if let Some(path) = changed
        .lines()
        .find(|path| RELEASE_GATE_PATHS.iter().any(|p| path.starts_with(p)))
    {
        return Some(path.to_owned());
    }
    let old = git(&["show", &format!("{base}:Cargo.toml")]).unwrap_or_default();
    let new = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    if profile_lines(&old) != profile_lines(&new) {
        return Some("Cargo.toml's [profile.*]".to_owned());
    }
    let marks = git(&["diff", "-U0", base, "--", "crates"]).unwrap_or_default();
    marks
        .lines()
        .any(|l| (l.starts_with('+') || l.starts_with('-')) && l.contains("optimize(size)"))
        .then(|| "an optimize(size) mark".to_owned())
}

/// The lines of `toml`'s `[profile.*]` tables, without comments.
fn profile_lines(toml: &str) -> Vec<&str> {
    let mut inside = false;
    toml.lines()
        .map(str::trim)
        .filter(|line| {
            if line.starts_with('[') {
                inside = line.starts_with("[profile.");
            }
            inside && !line.is_empty() && !line.starts_with('#')
        })
        .collect()
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

/// The line by which a later head's size gate knows this xtask's `size
/// --head` builds its tree's shipped artifacts with its own recipe and
/// writes `<artifact> <bytes> <path>` lines to `target/size/head.tsv`
/// (`size.rs` greps a base's main.rs for it). Keep it while that holds.
pub(crate) const SIZE_HEAD: &str = "size --head: own recipe, target/size/head.tsv, paths";

/// Builds the shipped artifacts and checks their sizes against the base's
/// and the release's (`size.rs`).
///
/// `check` runs it in three stages, so the base's and the release's builds
/// (`--references`) run beside the head's (`--head`, which writes the
/// head's sizes to `target/size/head.tsv`) and `--judge` compares them.
fn size(args: &[String]) -> Result<(), String> {
    let (mode, args) = match args {
        [flag, rest @ ..] if flag.starts_with("--") && flag != "--base" => (flag.as_str(), rest),
        _ => ("", args),
    };
    let base = match args {
        [] => "origin/main",
        [flag, rev] if flag == "--base" => rev,
        _ => return Err(USAGE.to_owned()),
    };
    let root = workspace_root();
    let file = root.join("target/size/head.tsv");
    let head = match mode {
        "--references" => return size::gate(None, base, &shipped_sizes),
        "--head" => return size::write(&file, &shipped_artifacts(&root, &root.join("target"))?),
        "--judge" => size::read(&file)?,
        "" => shipped_sizes(&root, &root.join("target"))?
            .into_iter()
            .map(|(a, b)| (a.to_owned(), b))
            .collect(),
        _ => return Err(USAGE.to_owned()),
    };
    size::gate(Some(&head), base, &shipped_sizes)
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
