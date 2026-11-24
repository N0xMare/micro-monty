//! Repository tasks, run as `cargo xtask <task>`.
//!
//! - `size`: builds the facade's `hello` example as shipped (build-std,
//!   immediate abort, no unwind tables; `wasm-opt` on wasm) for
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
//!   `wasm32-unknown-unknown` (`wasm-opt` included), their sizes, and a
//!   snapshot round trip under wasmtime when it is installed.
//! - `wasm-opt`: binaryen's `wasm-opt` with the shipped arguments
//!   for a target (`wasm::wasm_opt_args`, their one owner) for scripts
//!   outside xtask.
//! - `wasm-wall`: wasm wall time of the bench programs against a base
//!   revision, on wasmtime and V8.
//! - `wasm-ir`: wasmtime's guest instructions on the bench programs
//!   against a base revision (callgrind; deterministic).
//! - `replay-ir`: the recorded real sessions' instructions on the replay
//!   worker against a base revision (callgrind).
//! - `gc-stress`: own cases, the corpus and durability on the own cases
//!   with `MM_GC_STRESS=1` (a collection at every safepoint).
//! - `handoffs`: the native-to-Python hand-off sites (`need_py()` calls)
//!   by file and function, written to (`--check`: compared with)
//!   `xtask/handoff-sites.txt`.
//! - `overflow-wasip1`: every case on `mm` for `wasm32-wasip1` built with
//!   overflow checks, under wasmtime; fails on a panic or trap (release
//!   gate).
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

mod accept;
mod bench;
mod check;
mod conformance;
mod difffuzz;
mod docs;
mod durability;
mod floors;
mod gc_stress;
mod handoffs;
mod native_ir;
mod overflow;
mod package;
mod replay_ir;
mod replay_worker;
mod restack;
mod shipped;
mod size;
mod tag_reference;
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
      --release-gate         every stage, release builds, durability at
                             1:1000: on main's tip after pushes and before
                             a tag (default: the dev tier, which runs fmt,
                             clippy, docs, test, features, conformance
                             natively, durability at 1:100, golden, size and
                             native-ir, plus the stages that guard what the
                             change touches; or the release gate when it
                             touches the shipped recipe, the release-gate
                             Ir gates, the toolchain, Cargo.lock, .cargo/,
                             a profile or an optimize mark; AGENTS.md)
      --base <rev>           the revision the change is measured from: the
                             tier's paths, size and the Ir gates (default
                             origin/main; a stack's parent while the parent
                             is gated or waits to be pushed)
      --ir-base <tag>        the revision the four Ir gates measure from,
                             where it is not --base's: a `v*` tag, for the
                             commit that seeds the floors late (size and
                             the tier's paths stay on --base)
  size [--base <rev>]        build `hello` and the mm-wasm guests as shipped
                             (wasm-opt on wasm), and the base's (default
                             origin/main) and the release's, each with its
                             own toolchain in a worktree outside the checkout
                             (cached per commit); a rise
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
  shipped-mm [--target <t>]  build `mm` as shipped, natively (the binary the
                             bench Ir rule measures) or for <t> (wasm-opt -Oz
                             on wasm), and print its path
  shipped-build --target <t> --package <p>
                             build a package's binaries as shipped for a
                             target (release.yml's published natives)
  wasm                       build the facade as shipped (wasm-opt) for
                             wasm32-wasip1 and wasm32-unknown-unknown, report
                             hello.wasm sizes, run a snapshot round trip under
                             wasmtime
  js-host                    hosts/js' tests (node 23.6 or later, no npm ci)
                             on the default and the bundle guest, as built
                             by `size --head`, with `wasm-host`'s mod-uuid
                             guest as the upgrade test's second build (in
                             the release gate)
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
  native-ir [options]        bench/*.py's gated callgrind Ir on the shipped
                             native mm ($MM, else built; empty environment,
                             fixed paths) against the base (built outside the
                             checkout with its own toolchain and recipe,
                             cached per commit) and the floors in
                             xtask/ir-floors/native-ir.txt: fails on a program
                             past +0.5% or startup past +300 instructions,
                             unless a `Native-IR-Accepted: <programs>
                             <ceiling> <reason>` line covers it, or on a
                             floor file that is not what the head makes of it
      --base <rev>           the revision to compare with (default
                             origin/main; at a v* tag the floors are checked
                             against it)
      --write-floors         write the floor file the head must have (at a
                             v* tag: seeded from the tag's counts)
      --plain                advisory: shipped against a plain release build
  replay-ir [options]        the 160 recorded real sessions' callgrind Ir on
                             the replay worker (shipped, --cfg mm_size_wasm)
                             against the base's and each session's floor in
                             xtask/ir-floors/replay-ir.txt (in the release
                             gate): fails on a session past +0.5% or the
                             median ratio past +0.3%, unless a
                             `Native-IR-Accepted: sessions <ceiling> <reason>`
                             line covers it; allocator calls apart
                             (`sessions-calls`)
      --base <rev>           the revision to compare with (default origin/main)
      --write-floors         write the floor file the head must have (at a
                             v* tag: seeded from the tag's counts)
  v8-ir [options]            V8's instructions (node --no-liftoff
                             --single-threaded, generated code, the second
                             of two runs) on bench/*.py in
                             both wasm32-unknown-unknown guests against a base
                             and the floors, as wasm-ir (in the release gate):
                             fails on a program rising past 0.5% or a guest's
                             geomean past 0.3%, unless a `V8-IR-Accepted:
                             <names> <ceiling> <reason>` line covers it
      --base <rev>           the revision to compare with (default origin/main)
      --programs <a,b>       only these programs
      --head-built           the head's guests from target/size/head.tsv
      --write-floors         write the floor file the head must have (at a
                             v* tag: seeded from the tag's counts)
  wasm-ir [options]          wasmtime guest instructions of bench/*.py against
                             a base revision and the floors under callgrind
                             (in check): fails on a program rising past 1.0%,
                             the geomean past 0.5% or startup past 300
                             instructions, unless a `Wasm-IR-Accepted:
                             <names> <ceiling> <reason>` line covers it
      --base <rev>           the revision to compare with (default origin/main)
      --head <rev>           a revision instead of the working tree
      --programs <a,b,..>    only these programs (default all)
      --baselines <file>     only the head, against the counts stored in
                             <file> (bench/baselines-0.0.2.txt); advisory
      --write-floors         write the floor file the head must have (at a
                             v* tag: seeded from the tag's counts)
  wasm-opt <target> <in> <out> [arg..]
                             binaryen's wasm-opt with <target>'s shipped
                             arguments (wasip1: -Oz and the inline cap;
                             unknown-unknown: -Oz), plus <arg>s
  gc-stress                  collect at every safepoint (MM_GC_STRESS=1): own
                             cases must print what they print without, the
                             corpus keep its results, durability on the own
                             cases pass (not in check: about 85 minutes)
  handoffs [--check]         write (--check: compare) xtask/handoff-sites.txt,
                             the need_py() hand-off sites by file and function
  restack <commit|a..b>...   cherry-pick commits onto HEAD, blessing the golden
                             images again in each commit that moves the
                             snapshot build id (the images' conflicts take
                             the parent's); stops at any other conflict and
                             where blessing would keep HEAD's id
      --resolved <commit>    the staged pick restack stopped at, resolved:
                             tested, blessed and committed, then the rest
  overflow-wasip1            every own, corpus, codemode and breadth case on mm
                             for wasm32-wasip1 built with overflow checks
                             (target/check/overflow-wasip1); fails on a panic
                             or trap
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
        Some("shipped-mm") => shipped_mm(&rest),
        Some("shipped-build") => shipped::shipped_build_cli(&rest),
        Some("wasm") => wasm::run(&rest),
        Some("wasm-host") if rest.is_empty() => wasm::host(),
        Some("js-host") if rest.is_empty() => wasm::js_host(),
        Some("wasm-wall") => wasm_wall::run(&rest),
        Some("wasm-ir") => wasm_ir::run(&rest),
        Some("native-ir") => native_ir::run(&rest),
        Some("replay-ir") => replay_ir::run(&rest),
        Some("v8-ir") => v8_ir::run(&rest),
        Some("wasm-opt") => wasm::wasm_opt_command(&rest),
        Some("gc-stress") => gc_stress::run(&rest),
        Some("package") if rest.is_empty() => package::run(),
        Some("docs") if rest.is_empty() => docs::run(),
        Some("difffuzz") => difffuzz::run(&rest),
        Some("handoffs") => handoffs::run(&rest),
        Some("restack") => restack::run(&rest),
        Some("overflow-wasip1") if rest.is_empty() => overflow::run(),
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

/// The local gates, as stages that run at the same time where they can
/// (`check.rs`): the dev tier ([`DEV_STAGES`] and what the change's paths
/// add, [`DEV_ADDS`]), or with `--release-gate` every stage.
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
/// runs at normal priority, the rest at `nice` 10. The cross-target
/// durability run comes after it: its heaviest `mm` runs at `nice` 10
/// beside durability's pass the oracle's 120 s wall limit. The step-time
/// oracle measures wall time, so it runs last, alone.
#[allow(clippy::too_many_lines, reason = "one flat list of stages")]
fn check(args: &[String]) -> Result<(), String> {
    let (mut serial, mut release_gate) = (false, false);
    let (mut base, mut ir_base) = (None, None);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--serial" => serial = true,
            "--release-gate" => release_gate = true,
            "--base" => base = Some(args.next().ok_or("check: --base needs a revision")?),
            "--ir-base" => {
                ir_base = Some(args.next().ok_or("check: --ir-base needs a revision")?);
            }
            other => return Err(format!("check: unknown option `{other}`\n{USAGE}")),
        }
    }
    let root = workspace_root();
    let head = wasm_wall::git(&root, &["rev-parse", "HEAD"])?;
    let merge_base = base.map_or("origin/main", String::as_str);
    let merge_base = wasm_wall::git(&root, &["merge-base", merge_base, "HEAD"]).ok();
    // An uncommitted change on the base is gated, as against origin/main.
    if let Some(given) = base
        && merge_base.as_deref() == Some(head.as_str())
        && wasm_wall::git(&root, &["status", "--porcelain"])?.is_empty()
    {
        return Err(format!(
            "check: --base {given} already holds HEAD ({head}) and the tree is clean, so \
             nothing would be measured; give the stack's parent"
        ));
    }
    let base = base.map_or("origin/main", String::as_str);
    println!(
        "check: measured from {base} (merge base {}), head {head}",
        merge_base.as_deref().unwrap_or("none")
    );
    // A seed that lands late: the Ir gates judge the floors against the
    // tag they are seeded from, which the base's floors cannot stand for
    // (stale ones name builds no commit has), while size and the tier's
    // paths judge the change since the base, as for any push.
    let ir_base = ir_base.map_or(base, String::as_str);
    if ir_base != base {
        println!("check: the Ir gates measure from {ir_base}");
    }
    // The files the change touches, for the stages its paths add.
    let changed = merge_base
        .as_deref()
        .map_or_default(|base| changed_files(&root, base));
    let mut adds = Vec::new();
    if !release_gate {
        if let Some(why) = release_gate_reason(&root, base) {
            println!("check: the release gate, as the change touches {why} (AGENTS.md)");
            release_gate = true;
        } else {
            adds = dev_adds(&changed);
            println!("check: the dev tier (AGENTS.md); --release-gate runs every stage");
            for (stages, path) in &adds {
                println!(
                    "check: also {}, as the change touches {path}",
                    stages.join(" and ")
                );
            }
        }
    }
    // Durability's fine setting with the cross-target run: in the release
    // gate, and in the dev tier for a change to what an image holds.
    let fine = release_gate
        || adds
            .iter()
            .any(|(stages, _)| stages.contains(&"durability-cross"));
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    // Stages run a copy of this binary, which no stage's build replaces.
    let xtask = root.join("target/check/xtask");
    let exe = env::current_exe().map_err(|e| format!("cannot find xtask's binary: {e}"))?;
    if let Some(dir) = xtask.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    fs::copy(&exe, &xtask).map_err(|e| format!("cannot copy {}: {e}", exe.display()))?;
    let debug = root.join("target/check/debug");
    // The dev tier builds what it only runs (no size or time measured)
    // with thin LTO (`gate`); the release gate builds it as the release
    // profile does. Both tiers run the tests at opt 1 with debug assertions
    // (`gate-test`, Cargo.toml); `just test` keeps the opt-0 run.
    let profile = if release_gate { "release" } else { "gate" };
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
    let musl = host.replace("-linux-gnu", "-linux-musl");
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
    let steps_cargo_in = |dir: &Path, args: &[&str]| {
        let mut command = command(cargo, args, Some(dir));
        command
            .env("RUSTC_BOOTSTRAP", "1")
            .env("RUSTFLAGS", shipped::SHIPPED_RUSTFLAGS)
            .env_remove("CARGO_ENCODED_RUSTFLAGS");
        command
    };
    let steps_cargo = |args: &[&str]| steps_cargo_in(&steps_dir, args);
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
        // The dev tier parks the own cases and the corpus subset at most
        // about 100 times each beyond `5:50`, still at every host call;
        // the fine setting, about 1,000 (xtask/src/durability.rs).
        Stage::new("durability")
            .after(&["mm"])
            .urgent()
            .run(mm_task(if fine {
                &["durability"]
            } else {
                &["durability", "--fine", durability::DEV_FINE]
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
            "gate-test",
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
            "gate-test",
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
        // builds, each in its own worktree and target directory outside
        // the checkout (`shipped::refs_dir`), beside the head's.
        Stage::new("size-references").run(task(&["size", "--references", "--base", base])),
        Stage::new("size-head")
            .after(&["mm"])
            .run(task(&["size", "--head"])),
        Stage::new("size")
            .after(&["size-head", "size-references"])
            .run(task(&["size", "--judge", "--base", base])),
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
        // After durability too, whatever the cache holds: beside it, at
        // `nice` 10, its heaviest cases ran past the 120 s wall limit [M].
        Stage::new("durability-cross")
            .after(&["conformance-wasip1", "durability"])
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
                    "gate-test",
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
    // wasmtime's guest instructions against the base and the floors
    // (its own worktrees and target directories, `shipped::refs_dir`).
    if cfg!(target_os = "linux") {
        stages.push(Stage::new("wasm-ir").run(task(&["wasm-ir", "--base", ir_base])));
        // The bench programs' gated Ir on the shipped native `mm` against
        // the base's and the floors (AGENTS.md's Ir rule).
        stages.push(Stage::new("native-ir").after(&["shipped-mm"]).run({
            let mut ir = task(&["native-ir", "--base", ir_base]);
            ir.env("MM", &shipped_mm);
            ir
        }));
        let musl_mm = root.join(format!("target/{musl}/release/mm"));
        let musl_worker = root.join(format!("target/{musl}/release/mm-cell-worker"));
        // The worker's protocol tests on the shipped musl worker, with
        // `MIMALLOC_USE_NUMA_NODES` set to `nodes` if given.
        let musl_worker_protocol = |nodes: Option<&str>| {
            let mut test = debug_cargo(&[
                "test",
                "--locked",
                "--profile",
                "gate-test",
                "--target",
                &musl,
                "--package",
                "mm-cell-worker",
                "--test",
                "protocol",
            ]);
            test.env("MM_CELL_WORKER", &musl_worker);
            if let Some(nodes) = nodes {
                test.env("MIMALLOC_USE_NUMA_NODES", nodes);
            }
            test
        };
        // musl, the main embedder's Linux target (README "Platforms"): the
        // string functions are musl's, `mm` and the worker allocate through
        // mimalloc (C, built with musl-gcc: .cargo/config.toml), the
        // binaries link static-pie, and the worker's seccomp filter meets
        // musl's and mimalloc's system calls. `mm` built there as shipped runs the own cases and the
        // corpus, and the worker's tests run there, the protocol suite
        // again on the worker built as shipped. The engine brings its
        // own libm, so durability and the other engine tests would repeat
        // the glibc stages. The step-time oracle runs there in the release
        // gate (`steps-musl`).
        stages.push(
            Stage::new("musl")
                .after(&["shipped-mm", "test"])
                .run({
                    let mut build = shipped_build(&musl);
                    build.args(["--package", "mm-cli", "--bin", "mm"]);
                    build
                })
                .run({
                    let mut test = debug_cargo(&[
                        "test",
                        "--locked",
                        "--profile",
                        "gate-test",
                        "--package",
                        "mm-cli",
                        "--test",
                        "conformance_own",
                    ]);
                    test.env("MM", &musl_mm);
                    test
                })
                .run({
                    let mut corpus = task(&["conformance", "--engine", "micro-monty", "--check"]);
                    corpus.env("MM", &musl_mm);
                    corpus
                })
                .run(debug_cargo(&[
                    "test",
                    "--locked",
                    "--profile",
                    "gate-test",
                    "--target",
                    &musl,
                    "--package",
                    "mm-cell-worker",
                ]))
                // The worker as release.yml publishes it (shipped, its
                // own build so `mm`'s features stay out of it), under its
                // filter: the protocol suite, as mimalloc runs on one
                // NUMA node and on two.
                .run({
                    let mut build = shipped_build(&musl);
                    build.args(["--package", "mm-cell-worker", "--bins"]);
                    build
                })
                .run(musl_worker_protocol(None))
                .run(musl_worker_protocol(Some("2"))),
        );
    } else {
        println!(
            "check: wasm-ir and native-ir skipped (valgrind runs on Linux only), and musl (a Linux target)"
        );
    }
    if cfg!(target_os = "linux") {
        // V8's instructions on the guests the size-head stage built.
        stages.push(Stage::new("v8-ir").after(&["size-head"]).run(task(&[
            "v8-ir",
            "--head-built",
            "--base",
            ir_base,
        ])));
        // The recorded real sessions' Ir on the shipped replay worker,
        // built beside shipped-mm's mm (the same target directory). Its
        // builds and replays fill cores for minutes, so it waits for the
        // stages with wall-clock limits; an Ir count does not care when it
        // runs.
        stages.push(
            Stage::new("replay-ir")
                .after(&[
                    "shipped-mm",
                    "durability",
                    "durability-cross",
                    "wasm-host",
                    "js-host",
                ])
                .run(task(&["replay-ir", "--base", ir_base])),
        );
    }
    {
        // Next to the other wasip1 stage, but off the critical path: it
        // needs no `mm`, builds in its own target directory and runs at
        // `nice` 10.
        let at = stages
            .iter()
            .position(|s| s.name == "conformance-wasip1")
            .map_or(stages.len(), |i| i + 1);
        stages.insert(
            at,
            Stage::new("overflow-wasip1").run(task(&["overflow-wasip1"])),
        );
        stages.push(
            Stage::new("wasm-host")
                .after(&["size"])
                .run(task(&["wasm-host"])),
        );
        // The JS host's tests on the guests `size-head` built, with the
        // second build `wasm-host` made for its own upgrade test: no build
        // of its own. Some of its tests bound wall time (a deadline, a
        // park from another thread), so replay-ir waits for it.
        stages.push(
            Stage::new("js-host")
                .after(&["size-head", "wasm-host"])
                .run(task(&["js-host"])),
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
    if release_gate && cfg!(target_os = "linux") {
        // The timed oracle on musl too, whose `memcpy`, `memcmp` and
        // `memmove` are slow where glibc's are not (README "Platforms"):
        // it guards the musl-only copy paths, such as `move_values`' stack
        // buffer, which a toolchain could fuse back into a `memmove`. In
        // the release gate only: run alone, the two runs add about 17 minutes.
        // Twice: on musl's `malloc` (`steps`), what an embedder without
        // an allocator gets, and on mimalloc (`steps_mimalloc`), which `mm`
        // and the worker register there. Their rows differ by up to 36 ms
        // either way [M]. `--no-fail-fast`: a failure on one allocator still
        // leaves the other judged in the same run.
        let mut musl_steps: Vec<&str> = steps
            .iter()
            .map(|&a| if a == host { musl.as_str() } else { a })
            .collect();
        musl_steps.extend(["--test", "steps_mimalloc"]);
        let musl_dir = root.join("target/check/steps-musl");
        stages.push(
            Stage::new("steps-musl-build")
                .after(&["steps-build"])
                .run(steps_cargo_in(
                    &musl_dir,
                    &[&musl_steps[..], &["--no-run"]].concat(),
                )),
        );
        stages.push(
            Stage::new("steps-musl")
                .after(&["steps-musl-build"])
                .alone()
                .run(steps_cargo_in(
                    &musl_dir,
                    &[
                        &musl_steps[..],
                        &[
                            "--no-fail-fast",
                            "--",
                            "--skip",
                            "slice_charges",
                            "--test-threads=1",
                        ],
                    ]
                    .concat(),
                )),
        );
    }
    if !release_gate {
        let mut kept = DEV_STAGES.to_vec();
        kept.extend(adds.iter().flat_map(|(stages, _)| stages.iter().copied()));
        // The Ir gates count under valgrind and musl is a Linux target:
        // Linux only.
        kept.retain(|name| cfg!(target_os = "linux") || !["native-ir", "musl"].contains(name));
        stages = check::keep(stages, &kept)?;
    }
    check::run(stages, &root, serial)
}

/// The dev tier's stages (`cargo xtask check` before a push): what failed
/// lanes most and needs no machine to itself. The release gate, on main's
/// tip after pushes and before a tag, runs every stage (AGENTS.md).
const DEV_STAGES: &[&str] = &[
    "fmt",
    "mm",
    "durability",
    "conformance",
    "conformance-mm",
    "test",
    "clippy",
    "features",
    "docs",
    "handoffs",
    "size-references",
    "size-head",
    "size",
    "shipped-mm",
    "golden",
    "native-ir",
];

/// Paths (prefixes) and the stages a change to one adds to the dev tier,
/// the ones that guard it: what an image holds (durability at its fine
/// setting too, `check`), the dispatch loop and the park state (the step
/// oracle's fuel half; its timed half needs the machine alone), the wasm
/// crates, and the worker (its musl build).
const DEV_ADDS: &[(&[&str], &[&str])] = &[
    (
        &[
            "crates/mm-engine/src/snapshot.rs",
            "crates/mm-core/src/gc",
            "crates/mm-core/src/snapshot.rs",
            "crates/micro-monty/src/image.rs",
            "crates/micro-monty/tests/golden/",
            "crates/mm-compile/src/encode.rs",
        ],
        &["durability-cross"],
    ),
    (
        &[
            "crates/mm-engine/src/interp.rs",
            "crates/mm-engine/src/nstep.rs",
            "crates/mm-engine/src/cont.rs",
            "crates/mm-engine/src/vm.rs",
            "crates/mm-engine/src/generator.rs",
            "crates/mm-engine/src/aio.rs",
        ],
        &["steps-build", "steps-fuel"],
    ),
    // The copies and compares that take another path on musl, where the
    // libc's are slow (`move_values`, `copy_bytes`, `bytes_eq`,
    // `push_char`). Their timed half, on musl too (`steps-musl`), stays in
    // the release gate.
    (
        &[
            "crates/mm-core/src/heap.rs",
            "crates/mm-core/src/string.rs",
            "crates/mm-engine/src/strmeth.rs",
        ],
        &["steps-build", "steps-fuel"],
    ),
    (&["crates/mm-wasm-"], &["wasm", "wasm-host"]),
    // The worker's seccomp rules for musl's and mimalloc's system calls
    // are compiled for musl only, with their tests: no other dev-tier
    // stage builds them.
    (&["crates/mm-cell-worker/"], &["musl"]),
];

/// The stages [`DEV_ADDS`] adds for `changed` (a path a line), each group
/// with the first path that asked for it.
fn dev_adds(changed: &str) -> Vec<(&'static [&'static str], &str)> {
    DEV_ADDS
        .iter()
        .filter_map(|(paths, stages)| {
            changed
                .lines()
                .find(|path| paths.iter().any(|p| path.starts_with(p)))
                .map(|path| (*stages, path))
        })
        .collect()
}

/// The files that differ from commit `base`, committed or not, new
/// untracked ones included, a path a line. `--no-renames`: a moved file
/// counts under its old and its new path, so `git mv interp.rs run.rs`
/// still names interp.rs.
fn changed_files(root: &Path, base: &str) -> String {
    let git = |args: &[&str]| wasm_wall::git(root, args).unwrap_or_default();
    git(&["diff", "--name-only", "--no-renames", base])
        + "\n"
        + &git(&["ls-files", "--others", "--exclude-standard"])
}

/// Paths (prefixes) whose change makes plain `check` the release gate:
/// what no dev-tier stage judges and every build or count depends on. How
/// shipped builds are made (`v8-ir` gates the guests), the gates that run
/// in the release gate only, and the toolchain, the lockfile and cargo's
/// configuration, which every build reads. Engine paths add their own
/// stages to the dev tier instead ([`DEV_ADDS`]).
const RELEASE_GATE_PATHS: &[&str] = &[
    "xtask/src/wasm.rs",
    "xtask/src/shipped.rs",
    // The code of the gates that run in the release gate only, and what
    // they judge with; and v8-ir's and replay-ir's floors, which only they
    // check.
    "xtask/src/v8_ir.rs",
    "xtask/src/replay_ir.rs",
    "xtask/src/replay_worker.rs",
    "xtask/src/tag_reference.rs",
    "xtask/tag-reference/",
    "xtask/src/accept.rs",
    "xtask/src/floors.rs",
    "xtask/ir-floors/v8-ir.txt",
    "xtask/ir-floors/replay-ir.txt",
    // What only they read: replay-ir's workload and lockstep driver, and
    // v8-ir's host JS, which every v8-ir key hashes.
    "conformance/codemode/real/gate.py",
    "conformance/codemode/real/carry.py",
    "conformance/codemode/real/traces.tar.xz",
    "hosts/js/src/",
    "hosts/js/bench/count.ts",
    // The JS host's tests, which only the release gate's `js-host` runs.
    "hosts/js/test/",
    "hosts/js/package.json",
    "rust-toolchain.toml",
    "Cargo.lock",
    ".cargo/",
    ".agents/setup",
];

/// Why the change since the merge base with `base_rev`, committed or
/// not (new untracked files included), needs the release gate: a path of [`RELEASE_GATE_PATHS`], Cargo.toml's
/// `[profile.*]` tables, an `optimize` mark (`size`, `speed` or `none`,
/// bare or in `cfg_attr`) in a `.rs` file under `crates/` added, removed,
/// edited or moved to another function, or no merge base to tell.
fn release_gate_reason(root: &Path, base_rev: &str) -> Option<String> {
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
    let Some(base) = git(&["merge-base", base_rev, "HEAD"]) else {
        return Some(format!(
            "an unknown set of files (no merge base with {base_rev})"
        ));
    };
    let base = base.trim();
    let changed = changed_files(root, base);
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
    // A changed line that names `optimize(` (a mark added, removed or
    // moved), or a file whose `optimize` attributes and the functions they
    // sit on differ (a condition edited on another line of a multi-line
    // `cfg_attr`, a function inserted under a mark or deleted from under
    // one).
    let diff = git(&[
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "-U0",
        base,
        "--",
        "crates",
    ])
    .unwrap_or_default();
    let mut file = "";
    let by_line = diff.lines().find_map(|l| {
        if let Some((_, b)) = l
            .strip_prefix("diff --git ")
            .and_then(|p| p.rsplit_once(" b/"))
        {
            file = b;
        }
        ((l.starts_with('+') || l.starts_with('-')) && names_optimize(l)).then_some(file)
    });
    let by_attr = || {
        changed
            .lines()
            .filter(|path| {
                path.starts_with("crates/")
                    && Path::new(path).extension().is_some_and(|e| e == "rs")
            })
            .find(|path| {
                let old = git(&["show", &format!("{base}:{path}")]).unwrap_or_default();
                let new = fs::read_to_string(root.join(path)).unwrap_or_default();
                optimize_attrs(&old) != optimize_attrs(&new)
            })
    };
    by_line
        .or_else(by_attr)
        .map(|path| format!("an optimize(...) mark in {path}"))
}

/// Whether `text` holds `optimize` followed by `(`, spaces allowed
/// between: every form of the attribute, and also prose that names it.
fn names_optimize(text: &str) -> bool {
    text.match_indices("optimize")
        .any(|(at, word)| text[at + word.len()..].trim_start().starts_with('('))
}

/// The attributes of `src` (`#[...]` and `#![...]`, brackets balanced)
/// that name `optimize(`, in order, without whitespace; an outer one with
/// the name of the function it marks, the next `fn` outside comments and
/// attributes. A `]` in a string inside an attribute ends it early.
pub(crate) fn optimize_attrs(src: &str) -> Vec<String> {
    let mut attrs = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find('#') {
        rest = &rest[at + 1..];
        let inner = rest.starts_with('!');
        let body = if inner { &rest[1..] } else { rest };
        if !body.starts_with('[') {
            continue;
        }
        let end = bracketed(body);
        rest = &body[end..];
        let mut attr: String = body[..end].split_whitespace().collect();
        if !names_optimize(&attr) {
            continue;
        }
        if let Some(name) = next_fn(rest).filter(|_| !inner) {
            attr = format!("{attr} fn {name}");
        }
        attrs.push(attr);
    }
    attrs
}

/// The length of the `[...]` that `body` starts with, brackets balanced,
/// or all of `body` when it is not closed.
fn bracketed(body: &str) -> usize {
    let mut depth = 0usize;
    body.char_indices()
        .find(|&(_, c)| {
            depth = match c {
                '[' => depth + 1,
                ']' => depth - 1,
                _ => depth,
            };
            depth == 0
        })
        .map_or(body.len(), |(i, _)| i + 1)
}

/// The name after the first `fn` keyword of `src` outside line comments,
/// block comments and attributes, so a comment or a `reason = "..."`
/// between a mark and its function does not supply the name.
fn next_fn(mut src: &str) -> Option<&str> {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut after_ident = false;
    while let Some(c) = src.chars().next() {
        if src.starts_with("//") {
            src = src.find('\n').map_or("", |i| &src[i..]);
        } else if src.starts_with("/*") {
            src = src.find("*/").map_or("", |i| &src[i + 2..]);
        } else if let Some(body) = src
            .strip_prefix("#!")
            .or_else(|| src.strip_prefix('#'))
            .filter(|body| body.starts_with('['))
        {
            src = &body[bracketed(body)..];
        } else if !after_ident
            && let Some(name) = src
                .strip_prefix("fn")
                .and_then(|s| s.strip_prefix(char::is_whitespace))
        {
            return name.trim_start().split(|c| !ident(c)).next();
        } else {
            after_ident = ident(c);
            src = &src[c.len_utf8()..];
            continue;
        }
        after_ident = false;
    }
    None
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

#[cfg(test)]
mod tests {
    use super::{dev_adds, names_optimize, optimize_attrs};

    #[test]
    fn a_change_adds_the_stages_that_guard_its_paths_to_the_dev_tier() {
        let stages = |changed: &str| -> Vec<&str> {
            dev_adds(changed)
                .iter()
                .flat_map(|(stages, _)| stages.iter().copied())
                .collect()
        };
        assert_eq!(
            stages("docs/spec.md\ncrates/mm-engine/src/ops.rs\n"),
            [] as [&str; 0]
        );
        assert_eq!(
            stages("crates/mm-engine/src/interp.rs\n"),
            ["steps-build", "steps-fuel"]
        );
        // The files of the musl-only copies and compares.
        assert_eq!(
            stages("crates/mm-core/src/heap.rs\ncrates/mm-engine/src/strmeth.rs\n"),
            ["steps-build", "steps-fuel"]
        );
        // `gc` is a prefix (gc.rs, gc_inc.rs); snapshot.rs of mm-engine is
        // an image path, not a dispatch one.
        assert_eq!(
            stages("crates/mm-core/src/gc_inc.rs\ncrates/mm-engine/src/snapshot.rs\n"),
            ["durability-cross"]
        );
        assert_eq!(
            stages("README.md\ncrates/mm-wasm-host/src/lib.rs\ncrates/mm-engine/src/vm.rs\n"),
            ["steps-build", "steps-fuel", "wasm", "wasm-host"]
        );
        // The worker's musl-only seccomp rules; `mm-cell-worker` is a
        // directory prefix, so another crate's file of that name is not one.
        assert_eq!(
            stages("crates/mm-cell-worker/src/seccomp.rs\ncrates/mm-cli/src/main.rs\n"),
            ["musl"]
        );
        assert_eq!(
            dev_adds("a\ncrates/mm-wasm-abi/x.rs\ncrates/mm-wasm-host/y.rs\n")[0].1,
            "crates/mm-wasm-abi/x.rs"
        );
    }

    #[test]
    fn a_mark_change_is_seen_by_the_attribute_scan() {
        let multi = "#[cfg_attr(\n    all(mm_shipped, not(target_arch = \"wasm32\")),\n    optimize(speed)\n)]\nfn f() {}\n";
        // (case, base, head, whether the scan sees a change)
        let rows: [(&str, &str, &str, bool); 9] = [
            (
                "single-line mark added",
                "fn f() {}",
                "#[optimize(speed)]\nfn f() {}",
                true,
            ),
            (
                "multi-line condition edited on its own line",
                multi,
                &multi.replace("not(target_arch = \"wasm32\")", "target_arch = \"wasm32\""),
                true,
            ),
            (
                "multi-line, reformatted only",
                multi,
                &multi.replace("\n    ", " "),
                false,
            ),
            (
                "inner mark added",
                "fn f() {}",
                "#![optimize(size)]\nfn f() {}",
                true,
            ),
            (
                "feature(optimize_attribute) edited",
                "#![cfg_attr(mm_shipped, feature(optimize_attribute))]",
                "#![cfg_attr(all(mm_shipped), feature(optimize_attribute))]",
                false,
            ),
            (
                "`optimize (size)` added",
                "fn f() {}",
                "#[optimize (size)]\nfn f() {}",
                true,
            ),
            (
                "unterminated `#[` added",
                "fn f() {}",
                "fn f() {}\n#[optimize(size)",
                true,
            ),
            (
                "nested `[..]` in the attribute, size to speed",
                "#[cfg_attr(all(a, doc = [\"x\"][0]), optimize(size))]\nfn f() {}",
                "#[cfg_attr(all(a, doc = [\"x\"][0]), optimize(speed))]\nfn f() {}",
                true,
            ),
            (
                "known miss: a `]` in a string before `optimize(`",
                "#[cfg_attr(all(a, feature = \"x]\"), optimize(size))]\nfn f() {}",
                "#[cfg_attr(all(b, feature = \"x]\"), optimize(size))]\nfn f() {}",
                false,
            ),
        ];
        for (case, base, head, seen) in rows {
            assert_eq!(optimize_attrs(base) != optimize_attrs(head), seen, "{case}");
        }
        assert!(names_optimize("+    #[optimize (none)]"));
        assert!(names_optimize("-// optimize(size) on dt_ord"));
        assert!(!names_optimize(
            "+#![cfg_attr(mm_shipped, feature(optimize_attribute))]"
        ));
    }

    #[test]
    fn a_mark_is_paired_with_the_function_it_marks() {
        let marked = "#[cfg_attr(mm_shipped, optimize(size))]\nfn marked() {}\n\nfn plain() {}\n";
        // (case, base, head, whether the scan sees a change)
        let rows: [(&str, &str, &str, bool); 7] = [
            (
                "function inserted under a mark",
                marked,
                &marked.replace("fn marked", "fn inserted() {}\n\nfn marked"),
                true,
            ),
            (
                "marked function deleted, its mark falls on the next",
                marked,
                &marked.replace("fn marked() {}\n\n", ""),
                true,
            ),
            (
                "marked function's body edited",
                marked,
                &marked.replace("marked() {}", "marked() { g() }"),
                false,
            ),
            (
                "a comment holding `fn` under the mark, marked function deleted",
                &marked.replace("fn marked", "// Out of line: the fn is cold.\nfn marked"),
                &marked.replace("fn marked() {}\n\n", "// Out of line: the fn is cold.\n"),
                true,
            ),
            (
                "an attribute holding `fn` under the mark, function inserted",
                &marked.replace(
                    "fn marked",
                    "#[expect(x, reason = \"one fn per case\")]\nfn marked",
                ),
                &marked.replace(
                    "fn marked",
                    "#[expect(x, reason = \"one fn per case\")]\nfn inserted() {}\n\nfn marked",
                ),
                true,
            ),
            (
                "marked function renamed",
                marked,
                &marked.replace("fn marked", "fn renamed"),
                true,
            ),
            (
                "an inner mark kept, a function inserted after its function",
                "fn f() {\n    #![optimize(size)]\n}\n",
                "fn f() {\n    #![optimize(size)]\n}\n\nfn g() {}\n",
                false,
            ),
        ];
        for (case, base, head, seen) in rows {
            assert_eq!(optimize_attrs(base) != optimize_attrs(head), seen, "{case}");
        }
    }
}
