//! `cargo xtask conformance`: runs every case in `conformance/monty/` on an
//! engine, tags it from its header, and prints, writes or checks the results.
//!
//! Both engines run through `conformance/oracle.py`, one process per case,
//! which parses each case's header and expectation as Monty's datatest does
//! and reports one JSON object per case. On CPython it replicates the datatest's
//! CPython side; with `--engine micro-monty` it runs the case through
//! `mm exec --json` and checks the same expectations against mm's report.
//! Results go to `conformance/results/<engine>.jsonl` (deterministic: sorted,
//! no timings or paths) and `summary.md` (CPython) or `micro-monty.md`.
//! `--target wasm32-wasip1` runs micro-monty's `mm` built for wasm32-wasip1
//! under wasmtime instead and scores it against the native results, which
//! it never rewrites: one engine, one set of results on every target.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::{cpython_oracle_version, micro_monty_binary, python, workspace_root};

/// Per-case wall-clock limit, the datatest watchdog's soft deadline (for
/// micro-monty, the oracle's wall-clock backstop plus 20 s).
const CASE_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-case limit for `mm exec` (the oracle's `--timeout`, `--timeout`
/// overrides it), in seconds of mm's own CPU time, so a loaded machine does
/// not turn a slow case into a `timeout`. A process that does not run at
/// all is stopped after `MM_WALL_FACTOR` times as much wall-clock time
/// (`conformance/oracle.py`, `MM_WALL_FACTOR`); both are below
/// `CASE_TIMEOUT` so the oracle can report them.
const MM_TIMEOUT_SECS: u64 = 10;
/// `conformance/oracle.py`'s `MM_WALL_FACTOR`.
const MM_WALL_FACTOR: u64 = 6;

/// micro-monty statuses, in reporting order.
const MM_STATUSES: [&str; 6] = ["pass", "fail", "skip", "crash", "timeout", "error"];

/// The tags, in reporting order. Every case has exactly one primary tag.
const TAGS: [&str; 4] = ["cpython", "host", "monty-specific", "excluded"];

/// An engine the corpus can be scored on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Engine {
    /// CPython 3.14 through `conformance/oracle.py`, the reference.
    Cpython,
    /// micro-monty's `mm exec --json`, driven by `conformance/oracle.py`.
    MicroMonty,
}

impl Engine {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "cpython" => Ok(Self::Cpython),
            "micro-monty" => Ok(Self::MicroMonty),
            _ => Err(format!(
                "unknown engine `{name}`; expected cpython or micro-monty"
            )),
        }
    }

    /// Results file name under `conformance/results/`.
    fn results_name(self) -> &'static str {
        match self {
            Self::Cpython => "cpython-3.14.jsonl",
            Self::MicroMonty => "micro-monty.jsonl",
        }
    }

    /// Summary file name under `conformance/results/`.
    fn summary_name(self) -> &'static str {
        match self {
            Self::Cpython => "summary.md",
            Self::MicroMonty => "micro-monty.md",
        }
    }

    /// The command that regenerates the results.
    fn rerun(self) -> &'static str {
        match self {
            Self::Cpython => "cargo xtask conformance",
            Self::MicroMonty => "cargo xtask conformance --engine micro-monty",
        }
    }
}

/// Command-line options.
struct Options {
    engine: Engine,
    filter: Option<String>,
    check: bool,
    /// `--timeout S`: CPU seconds per `mm exec` (default `MM_TIMEOUT_SECS`).
    timeout: Option<u64>,
    /// `--target wasm32-wasip1`: micro-monty's `mm` built for wasm32-wasip1
    /// under wasmtime, scored against the native results.
    wasm: bool,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            engine: Engine::Cpython,
            filter: None,
            check: false,
            timeout: None,
            wasm: false,
        };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--check" => options.check = true,
                "--filter" => {
                    let value = args.next().ok_or("--filter needs a substring")?;
                    options.filter = Some(value.clone());
                }
                "--engine" => {
                    let value = args.next().ok_or("--engine needs a name")?;
                    options.engine = Engine::parse(value)?;
                }
                "--timeout" => {
                    let value = args.next().ok_or("--timeout needs seconds")?;
                    let seconds = value
                        .parse()
                        .map_err(|_| format!("--timeout: `{value}` is not a number of seconds"))?;
                    options.timeout = Some(seconds);
                }
                "--target" => match args.next().map(String::as_str) {
                    Some("wasm32-wasip1") => options.wasm = true,
                    _ => return Err("--target: only wasm32-wasip1".to_owned()),
                },
                other => return Err(format!("conformance: unknown option `{other}`")),
            }
        }
        if options.wasm && options.engine != Engine::MicroMonty {
            return Err("--target wasm32-wasip1 needs --engine micro-monty".to_owned());
        }
        if options.check && options.filter.is_some() {
            return Err("--check compares the whole corpus; drop --filter".to_owned());
        }
        Ok(options)
    }
}

/// What the engine runs a case with.
struct Runner {
    engine: Engine,
    /// The CPython 3.14 interpreter that runs the oracle (and tags cases).
    python: PathBuf,
    oracle: PathBuf,
    /// The `mm` binary, for `Engine::MicroMonty`, or with `wasmtime` the
    /// precompiled wasm32-wasip1 module.
    mm: Option<PathBuf>,
    /// wasmtime, which runs `mm` for `--target wasm32-wasip1`.
    wasmtime: Option<PathBuf>,
    /// Seconds one `mm exec` may take.
    mm_timeout: u64,
}

/// One case's outcome, as recorded and reported.
struct CaseResult {
    file: String,
    tags: Vec<&'static str>,
    /// `pass`, `fail`, `error`, `skip`, `timeout` or (micro-monty) `crash`.
    status: String,
    /// The datatest's failure kind, e.g. `CPython traceback`.
    kind: String,
    reason: String,
    /// micro-monty only: the first cause of a non-pass, e.g. `does not support classes`.
    category: String,
    expected: String,
    actual: String,
}

impl CaseResult {
    fn primary_tag(&self) -> &'static str {
        self.tags.first().copied().unwrap_or("cpython")
    }

    fn jsonl_line(&self) -> String {
        let category = if self.category.is_empty() {
            String::new()
        } else {
            format!(",\"category\":{}", Value::from(self.category.as_str()))
        };
        format!(
            "{{\"file\":{},\"tags\":{},\"status\":{},\"reason\":{}{category}}}",
            Value::from(self.file.as_str()),
            Value::from(self.tags.clone()),
            Value::from(self.status.as_str()),
            Value::from(self.reason.as_str()),
        )
    }
}

/// Entry point for `cargo xtask conformance`.
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = Options::parse(args)?;
    let root = workspace_root();
    let python = python();
    let version = cpython_oracle_version(&python)?;
    let (mm, wasmtime) = match options.engine {
        Engine::Cpython => (None, None),
        Engine::MicroMonty if options.wasm => {
            let (wasmtime, module) = crate::wasm::wasip1_mm()?;
            (Some(module), Some(wasmtime))
        }
        Engine::MicroMonty => (Some(micro_monty_binary()?), None),
    };
    let runner = Runner {
        engine: options.engine,
        python,
        oracle: root.join("conformance/oracle.py"),
        mm,
        wasmtime,
        mm_timeout: options.timeout.unwrap_or(MM_TIMEOUT_SECS),
    };
    let cases = list_cases(&root.join("conformance/monty"), options.filter.as_deref())?;
    if cases.is_empty() {
        return Err("no case matches the filter".to_owned());
    }
    let workers = thread::available_parallelism().map_or(1, usize::from);
    match &runner.mm {
        None => println!(
            "conformance: {} cases on CPython {version} ({}), {workers} workers",
            cases.len(),
            runner.python.display()
        ),
        Some(mm) => println!(
            "conformance: {} cases on micro-monty ({}), oracle on CPython {version}, \
             {workers} workers",
            cases.len(),
            mm.display()
        ),
    }
    let started = Instant::now();
    let results = run_all(&runner, &cases, workers)?;
    println!("ran in {:.1} s\n", started.elapsed().as_secs_f64());
    match runner.engine {
        Engine::Cpython => print!("{}", summary_text(&results)),
        Engine::MicroMonty => print!("{}", mm_summary_text(&results, options.filter.is_some())),
    }

    let jsonl: String = results.iter().map(|r| r.jsonl_line() + "\n").collect();
    let results_dir = root.join("conformance/results");
    let committed_path = results_dir.join(runner.engine.results_name());
    if options.check {
        let committed = fs::read_to_string(&committed_path)
            .map_err(|error| format!("cannot read {}: {error}", committed_path.display()))?;
        let shown = format!("conformance/results/{}", runner.engine.results_name());
        return compare(&committed, &jsonl, &shown, runner.engine.rerun());
    }
    if options.filter.is_some() || options.wasm {
        let given = if options.wasm { "--target" } else { "--filter" };
        println!("\n{given} given: results files left unchanged");
        return Ok(());
    }
    fs::create_dir_all(&results_dir)
        .map_err(|error| format!("cannot create {}: {error}", results_dir.display()))?;
    write(&committed_path, &jsonl)?;
    let summary = match runner.engine {
        Engine::Cpython => summary_markdown(&results, &version),
        Engine::MicroMonty => mm_summary_markdown(&results, &version),
    };
    write(&results_dir.join(runner.engine.summary_name()), &summary)?;
    println!(
        "\nwrote {} and {}",
        committed_path
            .strip_prefix(&root)
            .unwrap_or(&committed_path)
            .display(),
        runner.engine.summary_name()
    );
    Ok(())
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// The corpus files, sorted by name, optionally filtered by a substring.
fn list_cases(dir: &Path, filter: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let entries =
        fs::read_dir(dir).map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
    let mut cases = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot read {}: {error}", dir.display()))?
            .path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let is_python = path.extension().is_some_and(|extension| extension == "py");
        if is_python && filter.is_none_or(|filter| name.contains(filter)) {
            cases.push(path);
        }
    }
    cases.sort();
    Ok(cases)
}

/// Runs every case across `workers` threads; results come back sorted by file.
fn run_all(runner: &Runner, cases: &[PathBuf], workers: usize) -> Result<Vec<CaseResult>, String> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(cases.len()));
    thread::scope(|scope| {
        for _ in 0..workers.min(cases.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(case) = cases.get(index) else { break };
                    let result = run_case(runner, case);
                    if let Ok(mut results) = results.lock() {
                        results.push(result);
                    }
                }
            });
        }
    });
    let mut results = results
        .into_inner()
        .map_err(|_| "a worker thread panicked".to_owned())?;
    if results.len() != cases.len() {
        return Err(format!(
            "lost results: {} of {} cases reported",
            results.len(),
            cases.len()
        ));
    }
    results.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(results)
}

fn file_name(case: &Path) -> String {
    case.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Runs `oracle.py <case>` (with `--engine micro-monty --mm <mm>` for mm) and
/// turns its JSON line into a `CaseResult`.
fn run_case(runner: &Runner, case: &Path) -> CaseResult {
    let file = file_name(case);
    let error = |kind: &str, reason: String, actual: String| CaseResult {
        file: file.clone(),
        tags: tags_for(&file, None),
        status: if kind == "timeout" {
            "timeout"
        } else {
            "error"
        }
        .to_owned(),
        kind: kind.to_owned(),
        reason,
        category: String::new(),
        expected: String::new(),
        actual,
    };
    let mut command = Command::new(&runner.python);
    command.arg(&runner.oracle);
    if let Some(mm) = &runner.mm {
        command.args([
            "--engine",
            "micro-monty",
            "--timeout",
            &runner.mm_timeout.to_string(),
        ]);
        match &runner.wasmtime {
            Some(wasmtime) => command
                .arg("--wasm")
                .arg(mm)
                .arg("--wasmtime")
                .arg(wasmtime),
            None => command.arg("--mm").arg(mm),
        };
    }
    let child = command
        .arg(case)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(err) => {
            return error(
                "oracle",
                format!("cannot start the oracle: {err}"),
                String::new(),
            );
        }
    };
    let case_timeout = if runner.mm.is_some() {
        CASE_TIMEOUT.max(Duration::from_secs(runner.mm_timeout * MM_WALL_FACTOR + 20))
    } else {
        CASE_TIMEOUT
    };
    let (stdout, stderr, timed_out) = match wait_with_timeout(child, case_timeout) {
        Ok(output) => output,
        Err(err) => return error("oracle", err, String::new()),
    };
    if timed_out {
        return error(
            "timeout",
            format!("timed out after {} s", case_timeout.as_secs()),
            stderr,
        );
    }
    let Some(record) = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
    else {
        let tail: String = stderr.lines().rev().take(1).collect();
        return error(
            "oracle",
            format!("oracle printed no result: {tail}"),
            stderr,
        );
    };
    let text = |key: &str| record[key].as_str().unwrap_or_default().to_owned();
    CaseResult {
        tags: tags_for(&file, Some(&record["header"])),
        status: text("status"),
        kind: text("kind"),
        reason: text("reason"),
        category: text("category"),
        expected: text("expected"),
        actual: text("actual"),
        file,
    }
}

/// Waits for `child`, killing it after `timeout`. Returns stdout, stderr and
/// whether it timed out.
fn wait_with_timeout(
    mut child: Child,
    timeout: Duration,
) -> Result<(String, String, bool), String> {
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let started = Instant::now();
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > timeout => {
                timed_out = true;
                // Already exited, or killed now: either way `wait` reaps it.
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(error) => return Err(format!("cannot wait for the oracle: {error}")),
        }
    }
    let join = |reader: Option<thread::JoinHandle<String>>| {
        reader
            .map(|handle| handle.join().unwrap_or_default())
            .unwrap_or_default()
    };
    Ok((join(stdout), join(stderr), timed_out))
}

/// Reads a pipe to the end on its own thread, so a full pipe cannot block the child.
fn drain(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Tags from the file name and the header the oracle parsed:
/// - `excluded`: `refcount__*` (micro-monty has no refcounting);
/// - `monty-specific`: `xfail` includes `cpython`;
/// - `host`: needs Monty's host fixtures (`call-external` or `mount-fs`);
/// - `cpython`: everything else.
///
/// The first tag is the primary one; a non-`host` case that needs a host also
/// carries `host` second.
fn tags_for(file: &str, header: Option<&Value>) -> Vec<&'static str> {
    let flag = |key: &str| header.is_some_and(|h| h[key].as_bool() == Some(true));
    let needs_host = flag("call-external") || flag("mount-fs");
    let xfail_cpython = header.is_some_and(|h| {
        h["xfail"]
            .as_array()
            .is_some_and(|list| list.iter().any(|item| item.as_str() == Some("cpython")))
    });
    let primary = if file.starts_with("refcount__") {
        "excluded"
    } else if xfail_cpython {
        "monty-specific"
    } else if needs_host {
        "host"
    } else {
        "cpython"
    };
    let mut tags = vec![primary];
    if needs_host && primary != "host" {
        tags.push("host");
    }
    tags
}

/// Per-tag counts: (cases, pass, fail, other).
fn counts(results: &[CaseResult]) -> BTreeMap<&'static str, [usize; 4]> {
    let mut counts = BTreeMap::new();
    for result in results {
        let row: &mut [usize; 4] = counts.entry(result.primary_tag()).or_default();
        row[0] += 1;
        match result.status.as_str() {
            "pass" => row[1] += 1,
            "fail" => row[2] += 1,
            _ => row[3] += 1,
        }
    }
    counts
}

/// Failures worth reading: any non-pass outside `monty-specific`/`excluded`,
/// and `monty-specific` cases that pass on CPython (strict xfail broken).
fn notable(results: &[CaseResult]) -> impl Iterator<Item = &CaseResult> {
    results.iter().filter(|r| match r.primary_tag() {
        "cpython" | "host" => r.status != "pass",
        "monty-specific" => r.status == "pass",
        _ => r.status == "error" || r.status == "timeout",
    })
}

fn summary_text(results: &[CaseResult]) -> String {
    let counts = counts(results);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<16} {:>6} {:>6} {:>6} {:>6}",
        "tag", "cases", "pass", "fail", "other"
    );
    let mut total = [0usize; 4];
    for tag in TAGS {
        let row = counts.get(tag).copied().unwrap_or_default();
        let _ = writeln!(
            out,
            "{tag:<16} {:>6} {:>6} {:>6} {:>6}",
            row[0], row[1], row[2], row[3]
        );
        for (sum, value) in total.iter_mut().zip(row) {
            *sum += value;
        }
    }
    let _ = writeln!(
        out,
        "{:<16} {:>6} {:>6} {:>6} {:>6}",
        "total", total[0], total[1], total[2], total[3]
    );
    let notable: Vec<&CaseResult> = notable(results).collect();
    if !notable.is_empty() {
        let _ = writeln!(
            out,
            "\nfailures in cpython/host cases, and monty-specific cases passing on CPython:"
        );
        let width = notable.iter().map(|r| r.file.len()).max().unwrap_or(0);
        for result in &notable {
            let _ = writeln!(
                out,
                "  {:<width$}  {:<14}  {:<8}  {}",
                result.file,
                result.primary_tag(),
                result.status,
                result.reason
            );
        }
    }
    // A small (filtered) run shows each failure in full.
    if results.len() <= 10 {
        for result in results.iter().filter(|r| r.status != "pass") {
            let _ = writeln!(
                out,
                "\n{} ({}):\n--- expected\n{}\n--- actual\n{}",
                result.file, result.kind, result.expected, result.actual
            );
        }
    }
    out
}

fn summary_markdown(results: &[CaseResult], version: &str) -> String {
    let counts = counts(results);
    let mut out = String::from("# Conformance results: CPython 3.14\n\n");
    let _ = writeln!(
        out,
        "[M] `cargo xtask conformance` on CPython {version}, over the {} cases of \
         `conformance/monty/` (Monty `66fff6bf`). Per-case results are in \
         `cpython-3.14.jsonl`; `cargo xtask check` reruns the corpus with `--check` \
         and fails if they change. Regenerate both files with `cargo xtask conformance`.\n",
        results.len()
    );
    out.push_str(
        "Tags come from each case's file name and `# /// monty` header: `excluded` \
         (`refcount__*`), `monty-specific` (`xfail` includes `cpython`), `host` \
         (`call-external` or `mount-fs`, run on CPython through the datatest's \
         emulated fixtures) and `cpython` (the rest). Monty's own CI also runs these \
         cases on CPython 3.14.\n\n",
    );
    out.push_str("| Tag | Cases | Pass | Fail | Other |\n|---|---:|---:|---:|---:|\n");
    for tag in TAGS {
        let row = counts.get(tag).copied().unwrap_or_default();
        let _ = writeln!(
            out,
            "| `{tag}` | {} | {} | {} | {} |",
            row[0], row[1], row[2], row[3]
        );
    }
    let _ = writeln!(out, "| total | {} | | | |", results.len());
    out.push_str(
        "\n`Other` is `skip` (ref-counts expectations, which the datatest skips on \
         CPython), `error` or `timeout`.\n",
    );
    for (tag, title) in [
        ("cpython", "`cpython` cases failing on CPython 3.14"),
        ("host", "`host` cases failing on CPython 3.14"),
    ] {
        let failing: Vec<&CaseResult> = results
            .iter()
            .filter(|r| r.primary_tag() == tag && r.status != "pass")
            .collect();
        let _ = writeln!(out, "\n## {title} ({})\n", failing.len());
        if failing.is_empty() {
            continue;
        }
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        for result in &failing {
            *kinds.entry(result.kind.as_str()).or_default() += 1;
        }
        let by_kind: Vec<String> = kinds
            .iter()
            .map(|(kind, count)| format!("{kind}: {count}"))
            .collect();
        let _ = writeln!(out, "By kind: {}.\n", by_kind.join(", "));
        out.push_str("| Case | Kind | Reason |\n|---|---|---|\n");
        for result in failing {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} |",
                result.file,
                result.kind,
                markdown_escape(&result.reason)
            );
        }
    }
    let specific: Vec<&CaseResult> = results
        .iter()
        .filter(|r| r.primary_tag() == "monty-specific")
        .collect();
    let passing = specific.iter().filter(|r| r.status == "pass").count();
    let _ = writeln!(
        out,
        "\n## `monty-specific` cases ({})\n\nExpected to fail on CPython (strict \
         `xfail`); {passing} pass on 3.14.\n\n| Case | Status | Reason |\n|---|---|---|",
        specific.len()
    );
    for result in specific {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} |",
            result.file,
            result.status,
            markdown_escape(&result.reason)
        );
    }
    out
}

/// micro-monty per-tag counts, one column per `MM_STATUSES` entry plus the total first.
fn mm_counts(results: &[CaseResult]) -> BTreeMap<&'static str, [usize; 7]> {
    let mut counts = BTreeMap::new();
    for result in results {
        let row: &mut [usize; 7] = counts.entry(result.primary_tag()).or_default();
        row[0] += 1;
        let column = MM_STATUSES
            .iter()
            .position(|status| *status == result.status)
            .unwrap_or(MM_STATUSES.len() - 1);
        row[column + 1] += 1;
    }
    counts
}

/// The first cause of a case that did not pass or skip: the oracle's category,
/// else its status (a timeout or error the runner itself reported).
fn mm_cause(result: &CaseResult) -> &str {
    if result.category.is_empty() {
        &result.status
    } else {
        &result.category
    }
}

/// Non-passing, non-skipped cases grouped by first cause, largest group first.
fn mm_causes(results: &[CaseResult]) -> Vec<(&str, Vec<&CaseResult>)> {
    let mut causes: BTreeMap<&str, Vec<&CaseResult>> = BTreeMap::new();
    for result in results
        .iter()
        .filter(|r| r.status != "pass" && r.status != "skip")
    {
        causes.entry(mm_cause(result)).or_default().push(result);
    }
    let mut causes: Vec<_> = causes.into_iter().collect();
    causes.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
    causes
}

fn mm_summary_text(results: &[CaseResult], filtered: bool) -> String {
    let counts = mm_counts(results);
    let mut out = String::new();
    let _ = write!(out, "{:<16} {:>6}", "tag", "cases");
    for status in MM_STATUSES {
        let _ = write!(out, " {status:>7}");
    }
    out.push('\n');
    let mut total = [0usize; 7];
    for tag in TAGS {
        let row = counts.get(tag).copied().unwrap_or_default();
        let _ = write!(out, "{tag:<16} {:>6}", row[0]);
        for value in &row[1..] {
            let _ = write!(out, " {value:>7}");
        }
        out.push('\n');
        for (sum, value) in total.iter_mut().zip(row) {
            *sum += value;
        }
    }
    let _ = write!(out, "{:<16} {:>6}", "total", total[0]);
    for value in &total[1..] {
        let _ = write!(out, " {value:>7}");
    }
    out.push('\n');
    let causes = mm_causes(results);
    if !causes.is_empty() {
        let _ = writeln!(out, "\nfailures by first cause:");
        for (cause, cases) in &causes {
            let _ = writeln!(out, "  {:>5}  {cause}", cases.len());
        }
    }
    if filtered {
        let failing: Vec<&CaseResult> = results.iter().filter(|r| r.status != "pass").collect();
        let width = failing.iter().map(|r| r.file.len()).max().unwrap_or(0);
        if !failing.is_empty() {
            out.push('\n');
        }
        for result in &failing {
            let _ = writeln!(
                out,
                "  {:<width$}  {:<8}  {}",
                result.file, result.status, result.reason
            );
        }
        if results.len() <= 10 {
            for result in failing.iter().filter(|r| r.status != "skip") {
                let _ = writeln!(
                    out,
                    "\n{} ({}):\n--- expected\n{}\n--- actual\n{}",
                    result.file, result.kind, result.expected, result.actual
                );
            }
        }
    }
    out
}

fn mm_summary_markdown(results: &[CaseResult], version: &str) -> String {
    let counts = mm_counts(results);
    let mut out = String::from("# Conformance results: micro-monty\n\n");
    let _ = writeln!(
        out,
        "[M] `cargo xtask conformance --engine micro-monty` over the {} cases of \
         `conformance/monty/` (Monty `66fff6bf`). Each case runs through `mm exec \
         --json` (release build) under `conformance/oracle.py` on CPython {version}, \
         which checks the case's expectation the way Monty's datatest checks its Monty \
         side (see `conformance/README.md`). Per-case results are in \
         `micro-monty.jsonl`; `cargo xtask check` reruns the corpus with `--check` and \
         fails on any change: a newly failing case is a regression, and a newly passing \
         one has to be recorded by regenerating both files with `cargo xtask conformance \
         --engine micro-monty`.\n",
        results.len()
    );
    out.push_str(
        "Tags are those of `summary.md`. `call-external` cases run with `mm exec \
         --host-fixtures` (mm's emulation of the datatest's host functions). Skipped: \
         `mount-fs` cases (they need the host filesystem), `excluded` cases \
         (`refcount__*`) and ref-counts expectations. \
         `crash` is a panic, signal or missing report from `mm`; `timeout` is a case \
         that used more than 10 s of mm's CPU time (or ran past 60 s).\n\n",
    );
    out.push_str("| Tag | Cases |");
    for status in MM_STATUSES {
        let _ = write!(out, " {} |", capitalise(status));
    }
    out.push_str("\n|---|---:|");
    out.push_str(&"---:|".repeat(MM_STATUSES.len()));
    out.push('\n');
    let mut total = [0usize; 7];
    for tag in TAGS {
        let row = counts.get(tag).copied().unwrap_or_default();
        let _ = write!(out, "| `{tag}` |");
        for value in row {
            let _ = write!(out, " {value} |");
        }
        out.push('\n');
        for (sum, value) in total.iter_mut().zip(row) {
            *sum += value;
        }
    }
    out.push_str("| total |");
    for value in total {
        let _ = write!(out, " {value} |");
    }
    out.push('\n');
    let causes = mm_causes(results);
    let failing: usize = causes.iter().map(|(_, cases)| cases.len()).sum();
    let _ = writeln!(
        out,
        "\n## Failures by first cause ({failing})\n\nThe first thing that stops each \
         case: a feature `mm` reports it does not support, a 64-bit int limit \
         (`bigint`), one int operation's work past about 50 ms (`int work cap`, \
         docs/spec.md §11), a builtin it lacks, else how the result differs (`wrong output`, \
         `wrong exception`, `wrong traceback`, `no exception raised`, `unexpected \
         <Type>`), or `crash`/`timeout`. Largest first, to order the milestones.\n\n\
         | Cause | Cases | Examples |\n|---|---:|---|"
    );
    for (cause, cases) in &causes {
        let examples: Vec<String> = cases
            .iter()
            .take(3)
            .map(|r| format!("`{}`", r.file))
            .collect();
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            markdown_escape(cause),
            cases.len(),
            examples.join(", ")
        );
    }
    let _ = writeln!(
        out,
        "\n## Failing cases ({failing})\n\n| Case | Tag | Status | Cause | Reason |\n\
         |---|---|---|---|---|"
    );
    for (cause, cases) in &causes {
        for result in cases {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} | {} |",
                result.file,
                result.primary_tag(),
                result.status,
                markdown_escape(cause),
                markdown_escape(&result.reason)
            );
        }
    }
    out
}

fn capitalise(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Escapes what Markdown would read as table cells, HTML, strikethrough or emphasis.
fn markdown_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '|' | '~' | '*' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '<' => out.push_str("&lt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Compares fresh results with the committed file and lists changed cases in
/// three groups: newly failing (a committed `pass` that no longer passes, a
/// regression), newly passing (an improvement, which must be recorded by
/// regenerating the results) and other changes (a different status or reason).
fn compare(committed: &str, fresh: &str, shown: &str, rerun: &str) -> Result<(), String> {
    if committed == fresh {
        println!("\ncheck: results match {shown}");
        return Ok(());
    }
    let index = |text: &str| -> BTreeMap<String, (String, String)> {
        text.lines()
            .map(|line| {
                let value = serde_json::from_str::<Value>(line).ok();
                let field = |key: &str| {
                    value
                        .as_ref()
                        .and_then(|value| value[key].as_str().map(str::to_owned))
                };
                let file = field("file").unwrap_or_else(|| line.to_owned());
                let status = field("status").unwrap_or_default();
                (file, (status, line.to_owned()))
            })
            .collect()
    };
    let (old, new) = (index(committed), index(fresh));
    let files: std::collections::BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    let (mut regressions, mut improvements, mut changed) =
        (String::new(), String::new(), String::new());
    let mut counts = [0usize; 3];
    for file in files {
        let (before, after) = (old.get(file), new.get(file));
        if before.map(|(_, line)| line) == after.map(|(_, line)| line) {
            continue;
        }
        let was_pass = before.is_some_and(|(status, _)| status == "pass");
        let is_pass = after.is_some_and(|(status, _)| status == "pass");
        let (group, count) = match (was_pass, is_pass) {
            (true, false) => (&mut regressions, &mut counts[0]),
            (false, true) => (&mut improvements, &mut counts[1]),
            _ => (&mut changed, &mut counts[2]),
        };
        *count += 1;
        if let Some((_, line)) = before {
            let _ = writeln!(group, "  - {line}");
        }
        if let Some((_, line)) = after {
            let _ = writeln!(group, "  + {line}");
        }
    }
    let mut report = String::new();
    for (title, count, lines) in [
        ("newly failing (regressions)", counts[0], &regressions),
        (
            "newly passing (improvements: regenerate the results to record them)",
            counts[1],
            &improvements,
        ),
        ("changed status or reason", counts[2], &changed),
    ] {
        if count > 0 {
            let _ = writeln!(report, "{title}: {count}\n{lines}");
        }
    }
    Err(format!(
        "results differ from {shown} (- committed, + now): {} newly failing, {} newly \
         passing, {} changed\n\n{report}rerun `{rerun}` and commit the results if the \
         change is intended",
        counts[0], counts[1], counts[2]
    ))
}
