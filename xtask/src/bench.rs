//! `cargo xtask bench`: times the docs/spec.md §12 programs in `bench/`.
//!
//! Every repetition is a fresh process timed by wall clock from spawn to exit,
//! so startup counts (as with hyperfine). Each run's stdout must equal the
//! program's line in `bench/expected.txt`. Runs are sequential and, when
//! `taskset` and a third CPU exist, pinned to CPU 2. Results go to
//! `bench/results/`, or `--out-dir`, with raw samples and a run manifest.
//!
//! Failures are never inferred to be unsupported features. Timed runs retain
//! partial results but fail the gate; quick runs fail immediately. Only engines
//! in the current invocation appear in its summary.

use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use crate::{command_output, cpython_oracle_version, micro_monty_binary, python, workspace_root};

/// Benchmark names in reporting order, with their file stem in `bench/`.
/// `json` lives in `json_rows.*` so the script cannot shadow the stdlib module.
pub(crate) const PROGRAMS: [(&str, &str); 13] = [
    ("startup", "startup"),
    ("fib", "fib"),
    ("loop", "loop"),
    ("dict", "dict"),
    ("str", "str"),
    ("json", "json_rows"),
    ("closure", "closure"),
    ("classes", "classes"),
    ("generators", "generators"),
    ("exceptions", "exceptions"),
    ("fmt", "fmt"),
    ("sort", "sort"),
    ("interp", "interp"),
];

/// The CPU benchmark processes are pinned to.
const PINNED_CPU: &str = "2";

/// An engine the programs can run on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Engine {
    /// CPython 3.14, required: `$PYTHON`, `~/.local/bin/python3.14` or `python3.14`.
    Cpython,
    /// QuickJS, optional: `$QJS` or `qjs` on `PATH`; runs the `.js` twins.
    Qjs,
    /// dquickts JavaScript, distinct from stock QuickJS.
    Dquickts,
    /// dquickts Python (`feat/py`).
    Qpy,
    /// djs, through a script-file CLI adapter if necessary.
    Djs,
    /// Monty's CLI, optional: `$MONTY` or `monty` on `PATH`.
    Monty,
    /// micro-monty's `mm run`: `$MM`, else `target/release/mm` built by cargo.
    MicroMonty,
    /// An independently built accepted-main control, supplied as `$MM_MAIN`.
    MicroMontyMain,
}

const ALL_ENGINES: [Engine; 8] = [
    Engine::Cpython,
    Engine::Qjs,
    Engine::Dquickts,
    Engine::Qpy,
    Engine::Djs,
    Engine::Monty,
    Engine::MicroMonty,
    Engine::MicroMontyMain,
];

impl Engine {
    fn parse(name: &str) -> Result<Self, String> {
        ALL_ENGINES
            .into_iter()
            .find(|engine| engine.name() == name)
            .ok_or_else(|| {
                format!("unknown engine `{name}`; expected cpython, qjs, dquickts, qpy, djs, monty, micro-monty or micro-monty-main")
            })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Cpython => "cpython",
            Self::Qjs => "qjs",
            Self::Dquickts => "dquickts",
            Self::Qpy => "qpy",
            Self::Djs => "djs",
            Self::Monty => "monty",
            Self::MicroMonty => "micro-monty",
            Self::MicroMontyMain => "micro-monty-main",
        }
    }

    /// File extension of the programs this engine runs.
    fn extension(self) -> &'static str {
        if matches!(self, Self::Qjs | Self::Dquickts | Self::Djs) {
            "js"
        } else {
            "py"
        }
    }

    /// Finds the engine's binary and version, or says why it is skipped.
    fn locate(self) -> Result<Located, String> {
        match self {
            Self::Cpython => {
                let binary = python();
                let version = format!("CPython {}", cpython_oracle_version(&binary)?);
                Ok(Located {
                    engine: self,
                    binary,
                    version,
                })
            }
            Self::Qjs | Self::Dquickts | Self::Qpy | Self::Djs => {
                let (var, name) = match self {
                    Self::Qjs => ("QJS", "qjs"),
                    Self::Dquickts => ("DQUICKTS", "dquickts"),
                    Self::Qpy => ("QPY", "qpy"),
                    _ => ("DJS", "djs"),
                };
                let binary = from_env_or_path(var, name)?;
                // `qjs -h` prints "QuickJS version <date>" first and exits non-zero.
                let output = Command::new(&binary)
                    .arg("-h")
                    .output()
                    .map_err(|error| format!("cannot run {}: {error}", binary.display()))?;
                let text = String::from_utf8_lossy(&output.stdout);
                let version = format!(
                    "{} {}",
                    self.name(),
                    text.lines().next().unwrap_or("unknown").trim()
                );
                Ok(Located {
                    engine: self,
                    binary,
                    version,
                })
            }
            Self::Monty => {
                let binary = from_env_or_path("MONTY", "monty")?;
                let version = command_output(&binary, &["--version"])?;
                Ok(Located {
                    engine: self,
                    binary,
                    version,
                })
            }
            Self::MicroMonty | Self::MicroMontyMain => {
                let binary = if self == Self::MicroMonty {
                    micro_monty_binary()?
                } else {
                    from_env_or_path("MM_MAIN", "mm-main")?
                };
                let version = format!("{} {}", self.name(), env!("CARGO_PKG_VERSION"));
                Ok(Located {
                    engine: self,
                    binary,
                    version,
                })
            }
        }
    }

    /// Arguments before the program path.
    fn command_args(self) -> &'static [&'static str] {
        if matches!(self, Self::MicroMonty | Self::MicroMontyMain | Self::Djs) {
            &["run"]
        } else if self == Self::Qpy {
            &["-B"]
        } else {
            &[]
        }
    }
}

/// A runnable engine.
struct Located {
    engine: Engine,
    binary: PathBuf,
    version: String,
}

/// `$var` if set, else `name` when it is on `PATH`.
fn from_env_or_path(var: &str, name: &str) -> Result<PathBuf, String> {
    if let Some(value) = env::var_os(var).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!(
                "${var} is {}, which does not exist",
                path.display()
            ))
        };
    }
    env::var_os("PATH")
        .iter()
        .flat_map(env::split_paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("not found: set ${var} or put `{name}` on PATH"))
}

/// Command-line options.
struct Options {
    engines: Vec<Engine>,
    reps: usize,
    quick: bool,
    required: bool,
    output: Option<PathBuf>,
    timeout: Option<u64>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            engines: vec![
                Engine::Cpython,
                Engine::Qjs,
                Engine::Monty,
                Engine::MicroMonty,
            ],
            reps: 10,
            quick: false,
            required: false,
            output: None,
            timeout: None,
        };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--quick" => options.quick = true,
                "--reps" => {
                    let value = args.next().ok_or("--reps needs a number")?;
                    options.reps = value
                        .parse()
                        .ok()
                        .filter(|&reps| reps > 0)
                        .ok_or_else(|| format!("--reps: `{value}` is not a positive number"))?;
                }
                "--engines" => {
                    options.required = true;
                    let value = args.next().ok_or("--engines needs a list")?;
                    options.engines = value
                        .split(',')
                        .map(Engine::parse)
                        .collect::<Result<_, _>>()?;
                }
                "--out-dir" => {
                    options.output =
                        Some(PathBuf::from(args.next().ok_or("--out-dir needs a path")?));
                }
                "--timeout-secs" => {
                    let value = args.next().ok_or("--timeout-secs needs a number")?;
                    options.timeout = Some(
                        value
                            .parse()
                            .ok()
                            .filter(|&n| n > 0)
                            .ok_or("--timeout-secs needs a positive integer")?,
                    );
                }
                other => return Err(format!("bench: unknown option `{other}`")),
            }
        }
        if options.quick {
            options.reps = 1;
        }
        Ok(options)
    }
}

/// Timings of one program on one engine, in milliseconds.
struct Timing {
    name: &'static str,
    median_ms: f64,
    min_ms: f64,
}

struct Results {
    engine: Located,
    timings: Vec<Timing>,
    failures: Vec<(&'static str, String)>,
}

struct Sample {
    elapsed_ms: f64,
    stdout: String,
    stderr: String,
    status: ExitStatus,
}

/// Entry point for `cargo xtask bench`.
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = Options::parse(args)?;
    let root = workspace_root();
    let bench_dir = root.join("bench");
    let expected = read_expected(&bench_dir.join("expected.txt"))?;
    let pin = pinning();
    let machine = machine_info(pin.is_some());
    if !options.quick {
        println!("machine: {machine}");
    }
    let mut skipped = Vec::new();
    let mut results = Vec::new();
    for engine in &options.engines {
        let located = match engine.locate() {
            Ok(located) => located,
            Err(reason)
                if options.required || matches!(engine, Engine::Cpython | Engine::MicroMonty) =>
            {
                return Err(format!("{} is required: {reason}", engine.name()));
            }
            Err(reason) => {
                println!("{}: skipped ({reason})", engine.name());
                skipped.push((engine.name(), reason));
                continue;
            }
        };
        println!("{}: {}", engine.name(), located.version);
        results.push(Results {
            engine: located,
            timings: Vec::new(),
            failures: Vec::new(),
        });
    }
    if results.is_empty() {
        println!("no engine ran; results files left unchanged");
        return Ok(());
    }
    let output = options
        .output
        .as_ref()
        .map_or_else(|| bench_dir.join("results"), |p| root.join(p));
    let mut raw = if options.quick {
        None
    } else {
        fs::create_dir_all(&output).map_err(|e| e.to_string())?;
        write_manifest(&output, &machine, &options, &results)?;
        Some(fs::File::create(output.join("samples.jsonl")).map_err(|e| e.to_string())?)
    };
    for (name, stem) in PROGRAMS {
        let want = expected
            .get(name)
            .ok_or_else(|| format!("bench/expected.txt has no line for {name}"))?;
        measure_program(
            &mut results,
            &bench_dir,
            (name, stem),
            want,
            &options,
            pin.as_deref(),
            &mut raw,
        )?;
    }
    if options.quick {
        println!("quick: every checksum matched; results files left unchanged");
        return Ok(());
    }
    for result in &results {
        write_engine_results(
            &output.join(format!("{}.txt", result.engine.engine.name())),
            &machine,
            &result.engine,
            options.reps,
            &result.timings,
            &result.failures,
        )?;
    }
    let engines: Vec<Engine> = results.iter().map(|r| r.engine.engine).collect();
    let summary = summary_markdown(&output, &machine, &skipped, &engines)?;
    fs::write(output.join("summary.md"), summary).map_err(|e| e.to_string())?;
    println!("wrote {}", output.display());
    if let Some((_, error)) = results.iter().flat_map(|r| &r.failures).next() {
        return Err(format!(
            "benchmark failed; partial results retained: {error}"
        ));
    }
    Ok(())
}

fn measure_program(
    results: &mut [Results],
    bench_dir: &Path,
    (name, stem): (&'static str, &str),
    want: &str,
    options: &Options,
    pin: Option<&[String]>,
    raw: &mut Option<fs::File>,
) -> Result<(), String> {
    let mut samples = vec![Vec::new(); results.len()];
    let mut failed = vec![false; results.len()];
    let rounds = if options.quick { 1 } else { options.reps + 1 };
    for round in 0..rounds {
        // Balanced rotation avoids giving one engine every cold/late slot.
        for offset in 0..results.len() {
            let index = (round + offset) % results.len();
            if failed[index] {
                continue;
            }
            let engine = &results[index].engine;
            let program = bench_dir.join(format!("{stem}.{}", engine.engine.extension()));
            let measured = time_program(engine, &program, pin, options.timeout)?;
            let error =
                (!measured.status.success() || measured.stdout.trim_end() != want).then(|| {
                    format!(
                        "{name} on {}: expected `{want}`, got `{}` ({}): {}",
                        engine.version,
                        measured.stdout.trim_end(),
                        measured.status,
                        measured.stderr.trim_end()
                    )
                });
            if let Some(raw) = raw {
                let sample = serde_json::json!({
                    "engine": engine.engine.name(), "program": name, "round": round,
                    "warmup": round == 0, "elapsed_ms": measured.elapsed_ms,
                    "stdout": measured.stdout, "stderr": measured.stderr,
                    "exit_code": measured.status.code(), "exit_status": measured.status.to_string(),
                    "error": error,
                });
                writeln!(raw, "{sample}").map_err(|e| e.to_string())?;
                raw.flush().map_err(|e| e.to_string())?;
            }
            match error {
                None if round > 0 || options.quick => samples[index].push(measured.elapsed_ms),
                None => {}
                Some(error) if options.quick => return Err(error),
                Some(error) => {
                    println!("  {} {name}: FAILED: {error}", engine.engine.name());
                    results[index].failures.push((name, error));
                    failed[index] = true;
                }
            }
        }
    }
    for (index, samples) in samples.iter_mut().enumerate() {
        if failed[index] {
            continue;
        }
        samples.sort_by(f64::total_cmp);
        let middle = samples.len() / 2;
        let median_ms = if samples.len() % 2 == 0 {
            f64::midpoint(samples[middle - 1], samples[middle])
        } else {
            samples[middle]
        };
        let timing = Timing {
            name,
            median_ms,
            min_ms: samples.first().copied().unwrap_or(median_ms),
        };
        println!(
            "  {} {name:<10} median {median_ms:>8.3} ms",
            results[index].engine.engine.name()
        );
        results[index].timings.push(timing);
    }
    Ok(())
}

fn write_manifest(
    output: &Path,
    machine: &str,
    options: &Options,
    results: &[Results],
) -> Result<(), String> {
    let engines: Vec<_> = results.iter().map(|r| {
        serde_json::json!({
            "name": r.engine.engine.name(), "binary": r.engine.binary,
            "version": r.engine.version,
            "sha256": command_output(Path::new("sha256sum"), &[r.engine.binary.to_string_lossy().as_ref()]).ok(),
        })
    }).collect();
    let manifest = serde_json::json!({
        "schema": 1, "unix_time_ns": SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos().to_string(),
        "machine": machine, "reps": options.reps, "warmups": 1,
        "boundary": "spawn to exit; taskset and optional GNU timeout overhead included",
        "order": "program blocks; balanced rotating engine order each round",
        "timeout_secs": options.timeout,
        "head": command_output(Path::new("git"), &["rev-parse", "HEAD"]).ok(),
        "status": command_output(Path::new("git"), &["status", "--porcelain=v1"]).ok(),
        "rustc": command_output(Path::new("rustc"), &["-Vv"]).ok(),
        "engines": engines,
    });
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// `bench/expected.txt` as name → expected line.
pub(crate) fn read_expected(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| line.split_once(": "))
        .map(|(name, line)| (name.to_owned(), line.to_owned()))
        .collect())
}

/// `taskset -c 2` when `taskset` works and CPU 2 exists.
fn pinning() -> Option<Vec<String>> {
    let cpus = thread::available_parallelism().map_or(1, usize::from);
    let works = cpus > 2
        && Command::new("taskset")
            .args(["-c", PINNED_CPU, "true"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
    works.then(|| vec!["taskset".to_owned(), "-c".to_owned(), PINNED_CPU.to_owned()])
}

/// Runs and checks one real process. Warmup and round ordering belong to `run`.
fn time_program(
    engine: &Located,
    program: &Path,
    pin: Option<&[String]>,
    timeout: Option<u64>,
) -> Result<Sample, String> {
    let mut command = if let Some(seconds) = timeout {
        let mut command = Command::new("timeout");
        command.args(["--kill-after=1", &seconds.to_string()]);
        if let Some([taskset, rest @ ..]) = pin {
            command.arg(taskset).args(rest);
        }
        command.arg(&engine.binary);
        command
    } else {
        match pin {
            Some([taskset, rest @ ..]) => {
                let mut command = Command::new(taskset);
                command.args(rest).arg(&engine.binary);
                command
            }
            _ => Command::new(&engine.binary),
        }
    };
    command
        .args(engine.engine.command_args())
        .arg(program)
        .current_dir(program.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null());
    let started = Instant::now();
    let output = command
        .output()
        .map_err(|error| format!("cannot run {}: {error}", engine.binary.display()))?;
    let elapsed = started.elapsed();
    Ok(Sample {
        elapsed_ms: elapsed.as_secs_f64() * 1000.0,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        status: output.status,
    })
}

/// Machine description for the top of results files: CPU, cores, kernel, date, pinning.
fn machine_info(pinned: bool) -> String {
    let lscpu = command_output(Path::new("lscpu"), &[]).unwrap_or_default();
    let model = lscpu
        .lines()
        .find_map(|line| line.strip_prefix("Model name:"))
        .map_or("unknown CPU", str::trim)
        .to_owned();
    let nproc = command_output(Path::new("nproc"), &[]).unwrap_or_else(|_| "?".to_owned());
    let kernel = command_output(Path::new("uname"), &["-sr"]).unwrap_or_else(|_| "?".to_owned());
    let date = command_output(Path::new("date"), &["-u", "+%Y-%m-%d"]).unwrap_or_default();
    let pinning = if pinned {
        format!("pinned with taskset -c {PINNED_CPU}")
    } else {
        "not pinned (no taskset or fewer than 3 CPUs)".to_owned()
    };
    format!("{model}, nproc {nproc}, {kernel}; {date}; {pinning}")
}

fn write_engine_results(
    path: &Path,
    machine: &str,
    engine: &Located,
    reps: usize,
    timings: &[Timing],
    failures: &[(&str, String)],
) -> Result<(), String> {
    let mut out = String::new();
    let _ = writeln!(out, "# [M] machine: {machine}");
    let _ = writeln!(out, "# engine: {}", engine.version);
    let _ = writeln!(
        out,
        "# wall clock per process (spawn to exit), {reps} timed reps after 1 warm-up"
    );
    let _ = writeln!(out, "# program median_ms min_ms");
    for timing in timings {
        let _ = writeln!(
            out,
            "{} {:.1} {:.1}",
            timing.name, timing.median_ms, timing.min_ms
        );
    }
    for (name, reason) in failures {
        let _ = writeln!(out, "# failed {name}: {reason}");
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    }
    fs::write(path, out).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Summarises only files written by this invocation, never historical engines.
fn summary_markdown(
    results: &Path,
    machine: &str,
    skipped: &[(&str, String)],
    engines: &[Engine],
) -> Result<String, String> {
    let mut columns = Vec::new();
    for engine in engines {
        let path = results.join(format!("{}.txt", engine.name()));
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let version = text
            .lines()
            .find_map(|line| line.strip_prefix("# engine: "))
            .unwrap_or(engine.name())
            .to_owned();
        let rows: BTreeMap<String, (String, String)> = text
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                Some((
                    fields.next()?.to_owned(),
                    (fields.next()?.to_owned(), fields.next()?.to_owned()),
                ))
            })
            .collect();
        let unsupported: BTreeMap<String, String> = text
            .lines()
            .filter_map(|line| line.strip_prefix("# failed "))
            .filter_map(|line| line.split_once(": "))
            .map(|(name, reason)| (name.to_owned(), reason.to_owned()))
            .collect();
        columns.push((version, rows, unsupported));
    }
    if columns.is_empty() {
        return Err("no results in bench/results/ to summarise".to_owned());
    }
    let mut out = String::from("# Benchmarks\n\n");
    let _ = writeln!(
        out,
        "[M] `cargo xtask bench`: median / min wall-clock ms per process (startup \
         included), after one untimed warm-up run; balanced rotating engine order. Each engine's file records its rep \
         count. Checksums are checked on every run. Programs in `bench/` \
         (docs/spec.md §12). Failed cells have no timing. Only engines measured in this invocation appear.\n\nRun on {machine}.\n"
    );
    out.push_str("| Program |");
    for (version, _, _) in &columns {
        let _ = write!(out, " {version} |");
    }
    out.push_str("\n|---|");
    out.push_str(&"---:|".repeat(columns.len()));
    out.push('\n');
    for (name, _) in PROGRAMS {
        let _ = write!(out, "| {name} |");
        for (_, rows, unsupported) in &columns {
            match rows.get(name) {
                Some((median, min)) => {
                    let _ = write!(out, " {median} / {min} |");
                }
                None if unsupported.contains_key(name) => out.push_str(" failed |"),
                None => out.push_str(" — |"),
            }
        }
        out.push('\n');
    }
    for (version, _, unsupported) in &columns {
        if !unsupported.is_empty() {
            let _ = writeln!(
                out,
                "\nFailures on {version} (not inferred to be unsupported):\n"
            );
            for (name, reason) in unsupported {
                let _ = writeln!(out, "- {name}: `{reason}`");
            }
        }
    }
    if !skipped.is_empty() {
        out.push_str("\nSkipped in the last run:\n\n");
        for (engine, reason) in skipped {
            let _ = writeln!(out, "- {engine}: {reason}");
        }
    }
    Ok(out)
}
