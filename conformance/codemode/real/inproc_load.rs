//! LOCAL measurement harness (micro-monty Phase 2, never pushed): many cell
//! sessions in one process, the way plak's runner would hold them, driven
//! through plak's own `CellClient`.
//!
//! ```text
//! inproc_load MODE N TRACE_DIR OUT_JSONL
//! ```
//!
//! MODE is `monty` or `mm` (child workers: `PLAK_PY_HOST`, and for `mm` the
//! `PLAK_PY_HOST_IDENTITY` override) or `native` / `wasm` (`PLAK_PY_INPROC`
//! must name the same backend). N sessions start at once; each runs
//!
//! 1. an import cell (`json`, `re`, `datetime`, `collections`, `asyncio`),
//! 2. a host-call cell (200 sequential `await tool({})`),
//! 3. the cells of one recorded real `plak run` session (round robin over
//!    TRACE_DIR's `*/*/trace-*.jsonl`), answering each nested call with
//!    its recorded result,
//!
//! then they all wait at a barrier, the process's RSS (plus every child
//! worker's) is sampled, and they shut down. One JSON line per session goes
//! to OUT_JSONL; a summary line to stdout.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use plakous_cell::{
    BindingInput, CellClient, CellEvent, CellOutcome, CellPrelude, CellValue, EngineConfig,
    HostFrame, MONTY_WORKER, MontyBinding, MontyBootstrap, WorkerFrame,
};

/// One recorded cell: how it starts and its calls' recorded results.
#[derive(Clone)]
struct Cell {
    reset: bool,
    source: String,
    prelude: CellPrelude,
    results: Vec<(String, Result<CellValue, String>)>,
}

#[derive(Clone)]
struct Recorded {
    name: String,
    stubs: Option<String>,
    cells: Vec<Cell>,
}

fn load(path: &Path) -> Option<Recorded> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut stubs = None;
    let mut configured = false;
    let mut cells: Vec<Cell> = Vec::new();
    let mut calls: HashMap<u64, (usize, usize)> = HashMap::new();
    let mut reset = false;
    for line in text.lines() {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        let Some(frame) = value.get("f") else { continue };
        match value["d"].as_str()? {
            "h" => match serde_json::from_value::<HostFrame>(frame.clone()).ok()? {
                HostFrame::Configure { sdk_stubs, .. } => {
                    stubs = sdk_stubs;
                    configured = true;
                }
                HostFrame::Reset => reset = true,
                HostFrame::StartCell { source, prelude, .. } => {
                    cells.push(Cell {
                        reset: std::mem::take(&mut reset),
                        source,
                        prelude,
                        results: Vec::new(),
                    });
                }
                HostFrame::NestedResult { call, outcome, .. } => {
                    if let Some(&(cell, index)) = calls.get(&call) {
                        cells[cell].results[index].1 = outcome;
                    }
                }
                _ => {}
            },
            "w" => {
                if let Ok(WorkerFrame::NestedCall { call, name, .. }) =
                    serde_json::from_value::<WorkerFrame>(frame.clone())
                {
                    let cell = cells.len().checked_sub(1)?;
                    calls.insert(call, (cell, cells[cell].results.len()));
                    cells[cell]
                        .results
                        .push((name, Err("no recorded result".to_owned())));
                }
            }
            _ => {}
        }
    }
    (configured && !cells.is_empty()).then(|| Recorded {
        name: path.display().to_string(),
        stubs,
        cells,
    })
}

fn traces(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("trace-"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Runs one cell to its terminal: answers nested calls in order from
/// `results` (or with `"{}"` when there are none), each delivery on its own
/// task as plak's driver does. Returns the wall time, the call count and
/// whether the cell completed.
async fn run_cell(
    client: &Arc<CellClient>,
    cell: u64,
    source: &str,
    prelude: CellPrelude,
    results: &[(String, Result<CellValue, String>)],
) -> Result<(f64, usize, bool, String), String> {
    let started = Instant::now();
    client.start_cell(cell, source, prelude, None)?;
    let mut next = 0;
    let mut deliveries = Vec::new();
    loop {
        match client.read_event().await? {
            CellEvent::NestedCall { call, .. } => {
                let outcome = results
                    .get(next)
                    .map_or_else(|| Ok(CellValue::Str("{}".into())), |r| r.1.clone());
                next += 1;
                let client = Arc::clone(client);
                deliveries.push(tokio::spawn(async move {
                    client.deliver(cell, call, outcome).await
                }));
            }
            CellEvent::Done { outcome, .. } => {
                for delivery in deliveries {
                    let _ = delivery.await;
                }
                let (ok, message) = match outcome {
                    CellOutcome::Completed { .. } => (true, String::new()),
                    CellOutcome::Failed { message, .. } => (false, message),
                    CellOutcome::Stopped { .. } => (false, "stopped".into()),
                };
                return Ok((started.elapsed().as_secs_f64() * 1e3, next, ok, message));
            }
            _ => {}
        }
    }
}

fn tool_bootstrap() -> MontyBootstrap {
    MontyBootstrap {
        source: "async def tool(arguments):\n    return await _plakous_host_0(arguments)\n".into(),
        bindings: vec![MontyBinding {
            function: "_plakous_host_0".into(),
            tool: "tool".into(),
            input: BindingInput::Dictionary,
        }],
    }
}

const IMPORTS: &str = "import json, re, datetime, collections, asyncio\n";
const CALLS: &str = "for i in range(200):\n    await tool({})\n";

/// RSS of this process plus every child process, in KiB.
fn rss_kib() -> (u64, u64) {
    let field = |path: &str| {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|status| {
                status.lines().find_map(|line| {
                    line.strip_prefix("VmRSS:")?
                        .split_whitespace()
                        .next()?
                        .parse::<u64>()
                        .ok()
                })
            })
            .unwrap_or(0)
    };
    let own = field("/proc/self/status");
    let me = std::process::id().to_string();
    let mut children = 0;
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let pid = entry.file_name();
            let Some(pid) = pid.to_str().filter(|p| p.bytes().all(|b| b.is_ascii_digit())) else {
                continue;
            };
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            let ppid = stat
                .rsplit_once(')')
                .and_then(|(_, rest)| rest.split_whitespace().nth(1));
            if ppid == Some(me.as_str()) {
                children += field(&format!("/proc/{pid}/status"));
            }
        }
    }
    (own, children)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, mode, n, dir, out] = args.as_slice() else {
        eprintln!("usage: inproc_load MODE N TRACE_DIR OUT_JSONL");
        std::process::exit(2);
    };
    let n: usize = n.parse().expect("N");
    let program = PathBuf::from(std::env::var("PLAK_PY_HOST").unwrap_or_default());
    let recorded: Vec<Recorded> = traces(Path::new(dir)).iter().filter_map(|p| load(p)).collect();
    assert!(!recorded.is_empty(), "no traces under {dir}");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(16)
        .enable_all()
        .build()
        .expect("runtime");
    let mode = mode.clone();
    runtime.block_on(async move {
        // INPROC_WARM=1: one session imports and ends before the burst, as
        // in a runner that has served cells before (the per-process decoded
        // stdlib cache, if any, is then warm and counts in the baseline).
        if std::env::var_os("INPROC_WARM").is_some() {
            let client = CellClient::spawn(&program, MONTY_WORKER, EngineConfig::MONTY)
                .await
                .map(Arc::new)
                .expect("warm-up worker");
            let _ = run_cell(&client, 1, IMPORTS, CellPrelude::Monty { bootstrap: None }, &[]).await;
            let _ = client.terminate().await;
        }
        let (baseline, baseline_children) = rss_kib();
        // Each session reports idle (or dies, dropping its sender), then
        // waits for `go`: the RSS sample is taken between the two.
        let (idle_tx, mut idle_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let (go_tx, go_rx) = tokio::sync::watch::channel(false);
        let mut tasks = Vec::new();
        for index in 0..n {
            let session = recorded[index % recorded.len()].clone();
            let program = program.clone();
            let idle = idle_tx.clone();
            let mut go = go_rx.clone();
            tasks.push(tokio::spawn(async move {
                let spawned = Instant::now();
                let client = match &session.stubs {
                    Some(stubs) => CellClient::spawn_python_sdk(&program, stubs.clone()).await,
                    None => CellClient::spawn(&program, MONTY_WORKER, EngineConfig::MONTY).await,
                };
                let client = match client {
                    Ok(client) => Arc::new(client),
                    Err(error) => {
                        eprintln!("spawn failed: {error}");
                        let _ = idle.send(());
                        return serde_json::json!({"error": error.to_string()});
                    }
                };
                let spawn_ms = spawned.elapsed().as_secs_f64() * 1e3;
                let mut cell = 0;
                let mut run = |source: String, prelude: CellPrelude, results: Vec<_>| {
                    cell += 1;
                    let client = Arc::clone(&client);
                    let id = cell;
                    async move { run_cell(&client, id, &source, prelude, &results).await }
                };
                let imports = run(IMPORTS.into(), CellPrelude::Monty { bootstrap: None }, vec![]).await;
                // Defines the tool; `MontySdk` resets first, as plak's SDK does.
                let calls = run(
                    CALLS.into(),
                    if session.stubs.is_some() {
                        CellPrelude::MontySdk { bootstrap: tool_bootstrap() }
                    } else {
                        CellPrelude::Monty { bootstrap: Some(tool_bootstrap()) }
                    },
                    vec![],
                )
                .await;
                // A py-ptc session's bootstrap is installed once; the
                // replayed one installs its own, so it gets a fresh client.
                let replay_client = match &session.stubs {
                    Some(stubs) => CellClient::spawn_python_sdk(&program, stubs.clone()).await,
                    None => CellClient::spawn(&program, MONTY_WORKER, EngineConfig::MONTY).await,
                }
                .map(Arc::new);
                let mut cells = Vec::new();
                if let Ok(replay) = &replay_client {
                    for (index, recorded) in session.cells.iter().enumerate() {
                        let prelude = match (&recorded.prelude, recorded.reset) {
                            (CellPrelude::Monty { bootstrap: Some(b) }, true) => {
                                CellPrelude::MontySdk { bootstrap: b.clone() }
                            }
                            (prelude, _) => prelude.clone(),
                        };
                        let result = run_cell(
                            replay,
                            index as u64 + 1,
                            &recorded.source,
                            prelude,
                            &recorded.results,
                        )
                        .await;
                        cells.push(match result {
                            Ok((ms, calls, ok, _)) => {
                                serde_json::json!({"ms": ms, "calls": calls, "ok": ok})
                            }
                            Err(error) => serde_json::json!({"lost": error}),
                        });
                        if cells.last().is_some_and(|c| c.get("lost").is_some()) {
                            break;
                        }
                    }
                }
                let _ = idle.send(());
                // Held until every session is alive and idle: the RSS sample.
                let _ = go.wait_for(|go| *go).await;
                let _ = client.terminate().await;
                if let Ok(replay) = replay_client {
                    let _ = replay.terminate().await;
                }
                let cell = |r: Result<(f64, usize, bool, String), String>| match r {
                    Ok((ms, calls, ok, message)) => serde_json::json!({
                        "ms": ms, "calls": calls, "ok": ok,
                        "message": message.lines().last().unwrap_or_default()}),
                    Err(error) => serde_json::json!({"lost": error}),
                };
                serde_json::json!({
                    "session": session.name,
                    "sdk": session.stubs.is_some(),
                    "spawn_ms": spawn_ms,
                    "imports": cell(imports),
                    "calls": cell(calls),
                    "replay": cells,
                })
            }));
        }
        drop(idle_tx);
        let mut idle = 0;
        while idle < n && idle_rx.recv().await.is_some() {
            idle += 1;
        }
        let (rss, children) = rss_kib();
        go_tx.send_replace(true);
        let mut lines = Vec::new();
        for task in tasks {
            lines.push(task.await.unwrap_or_else(|e| serde_json::json!({"panic": e.to_string()})));
        }
        let mut file = String::new();
        for line in &lines {
            file.push_str(&line.to_string());
            file.push('\n');
        }
        std::fs::write(out, file).expect("write results");
        let median = |mut v: Vec<f64>| {
            v.sort_by(f64::total_cmp);
            v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
        };
        let pick = |key: &str| {
            median(lines.iter().filter_map(|l| l[key]["ms"].as_f64()).collect())
        };
        let replay: Vec<f64> = lines
            .iter()
            .flat_map(|l| l["replay"].as_array().cloned().unwrap_or_default())
            .filter_map(|c| c["ms"].as_f64())
            .collect();
        // Every session holds two clients (one replayed, one for the
        // import and call cells) at the sample.
        let per_session = (rss + children).saturating_sub(baseline + baseline_children) as f64
            / n as f64;
        println!(
            "{}",
            serde_json::json!({
                "mode": mode, "sessions": n,
                "spawn_ms_median": median(lines.iter().filter_map(|l| l["spawn_ms"].as_f64()).collect()),
                "import_cell_ms_median": pick("imports"),
                "calls_cell_ms_median": pick("calls"),
                "per_host_call_us_median": pick("calls") * 1e3 / 200.0,
                "replay_cell_ms_median": median(replay.clone()),
                "replay_cells": replay.len(),
                "rss_kib": rss, "children_rss_kib": children,
                "baseline_kib": baseline + baseline_children,
                "rss_per_session_kib": per_session,
                "failures": lines.iter().filter(|l| l["imports"]["ok"] != true || l["calls"]["ok"] != true || l.get("error").is_some()).count(),
            })
        );
        let _ = Duration::ZERO;
    });
}
