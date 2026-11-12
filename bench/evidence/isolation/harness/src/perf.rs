//! Performance [M]: one JSON object per line on stdout.

use std::{
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use iso_service::{Request, Response, ResumeMsg};
use serde_json::{Value as Json, json};

use crate::{
    Mode,
    backend::{Backend, ChildWorker, rss_pss_kib, self_rss_hwm_kib},
    drive::{self, CODEMODE, CODEMODE_CALLS, CODEMODE_OUT, Drive},
};

const MEMORY: u64 = 256 << 20;

fn emit(value: &Json) {
    println!("{value}");
}

fn us(d: Duration) -> f64 {
    d.as_secs_f64() * 1e6
}

/// Median and 99th percentile.
fn stats(mut xs: Vec<f64>) -> Json {
    xs.sort_by(f64::total_cmp);
    let pick = |q: f64| xs[((xs.len() - 1) as f64 * q).round() as usize];
    json!({"n": xs.len(), "median": pick(0.5), "p99": pick(0.99), "min": xs[0]})
}

/// The modes a section measures; `ISO_MODES` (comma-separated) narrows them.
fn modes_for(section: &str) -> Vec<&'static str> {
    let all = match section {
        "bench" => vec![
            "native",
            "wasm",
            "wasm-plain",
            "wasm-fuel",
            "abi",
            "abi-uu",
            "child",
        ],
        "warm" => vec!["native", "wasm", "wasm-pool", "abi", "abi-uu", "child"],
        "cold" => vec!["native", "wasm", "wasm-compile", "abi", "abi-uu", "child"],
        "cross" => vec!["native", "wasm", "abi", "abi-uu", "child"],
        _ => vec!["native", "wasm", "abi", "abi-uu", "child"],
    };
    match std::env::var("ISO_MODES") {
        Ok(only) if !only.is_empty() => all
            .into_iter()
            .filter(|m| only.split(',').any(|o| o == *m))
            .collect(),
        _ => all,
    }
}

pub fn main(args: &[String]) -> Result<(), String> {
    let all = [
        "cold", "warm", "hostcall", "bench", "snapshot", "memory", "scale",
    ];
    let sections: Vec<&str> = if args.is_empty() {
        all.to_vec()
    } else {
        args.iter().map(String::as_str).collect()
    };
    for section in sections {
        match section {
            "cold" => cold_all()?,
            "warm" => warm()?,
            "hostcall" => hostcall()?,
            "bench" => bench()?,
            "snapshot" => snapshot()?,
            "memory" => memory()?,
            "scale" => scale()?,
            "cross" => cross()?,
            "soak" => soak()?,
            other => return Err(format!("unknown section {other}")),
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ cold

/// In a new process: the time from `main` to the first cell's result.
pub fn cold(mode: &str) -> Result<(), String> {
    let start = Instant::now();
    let m = Mode::parse(mode)?;
    let mode_ready = start.elapsed();
    let mut b = m.backend()?;
    drive::new_session(b.as_mut(), MEMORY)?;
    let out = drive::run(b.as_mut(), "print(1)", Drive::default())?;
    let total = start.elapsed();
    if drive::output(&out) != "1\n" {
        return Err(format!("cold: {out:?}"));
    }
    #[cfg(feature = "wasm")]
    let load = match &m {
        Mode::Wasm(host) => us(host.load),
        Mode::Abi(rt) => us(rt.load_time),
        _ => 0.0,
    };
    #[cfg(not(feature = "wasm"))]
    let load = 0.0;
    emit(
        &json!({"mode": mode, "first_cell_us": us(total), "mode_ready_us": us(mode_ready), "module_load_us": load}),
    );
    Ok(())
}

fn cold_all() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    for mode in modes_for("cold") {
        let (mut inside, mut outside, mut load) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..10 {
            let t = Instant::now();
            let out = Command::new(&exe)
                .args(["cold", mode])
                .output()
                .map_err(|e| e.to_string())?;
            outside.push(us(t.elapsed()));
            let v: Json = serde_json::from_slice(&out.stdout).map_err(|e| {
                format!("cold {mode}: {e}: {}", String::from_utf8_lossy(&out.stderr))
            })?;
            inside.push(v["first_cell_us"].as_f64().unwrap_or(0.0));
            load.push(v["module_load_us"].as_f64().unwrap_or(0.0));
        }
        emit(&json!({"section": "cold", "mode": mode,
            "first_cell_us_in_process": stats(inside),
            "process_spawn_to_exit_us": stats(outside),
            "module_load_us": stats(load)}));
    }
    Ok(())
}

// ------------------------------------------------------------------ warm

fn warm() -> Result<(), String> {
    for name in modes_for("warm") {
        let mode = Mode::parse(name)?;
        let reps = if name == "child" { 50 } else { 200 };
        let mut create = Vec::new();
        let mut first = Vec::new();
        for i in 0..=reps {
            let t = Instant::now();
            let mut b = mode.backend()?;
            drive::new_session(b.as_mut(), MEMORY)?;
            let made = t.elapsed();
            let out = drive::run(b.as_mut(), "x = 1", Drive::default())?;
            let done = t.elapsed();
            if !matches!(out.response, Response::Complete { .. }) {
                return Err(format!("warm {name}: {out:?}"));
            }
            if i > 0 {
                create.push(us(made));
                first.push(us(done));
            }
        }
        emit(
            &json!({"section": "warm", "mode": name, "what": "new container + session",
            "session_ready_us": stats(create), "first_cell_done_us": stats(first)}),
        );
    }
    // A new session inside one existing worker / instance.
    // (abi modes hold one session per instance, so they have no such row.)
    for name in modes_for("warm")
        .into_iter()
        .filter(|m| matches!(*m, "native" | "wasm" | "child"))
    {
        let mode = Mode::parse(name)?;
        let mut b = mode.backend()?;
        let mut xs = Vec::new();
        for i in 0..=200 {
            let t = Instant::now();
            drive::new_session(b.as_mut(), MEMORY)?;
            drive::run(b.as_mut(), "x = 1", Drive::default())?;
            if i > 0 {
                xs.push(us(t.elapsed()));
            }
        }
        emit(
            &json!({"section": "warm", "mode": name, "what": "new session in an existing container",
            "first_cell_done_us": stats(xs)}),
        );
    }
    Ok(())
}

// -------------------------------------------------------------- hostcall

fn hostcall() -> Result<(), String> {
    const N: u64 = 20_000;
    for name in modes_for("hostcall") {
        let mode = Mode::parse(name)?;
        let mut b = mode.backend()?;
        drive::new_session(b.as_mut(), MEMORY)?;
        let expected = format!("{}\n", N * (N - 1) / 2);
        let (mut host, mut py, mut cells) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..7 {
            let t = Instant::now();
            let out = drive::run(b.as_mut(), &drive::call_loop(N), Drive::default())?;
            host.push(us(t.elapsed()) / N as f64);
            if drive::output(&out) != expected || out.host_calls != N {
                return Err(format!("hostcall {name}: {out:?}"));
            }
            let t = Instant::now();
            let out = drive::run(b.as_mut(), &drive::py_loop(N), Drive::default())?;
            py.push(us(t.elapsed()) / N as f64);
            if drive::output(&out) != expected {
                return Err(format!("pyloop {name}: {out:?}"));
            }
        }
        for _ in 0..21 {
            let t = Instant::now();
            let out = drive::run(b.as_mut(), CODEMODE, Drive::default())?;
            cells.push(us(t.elapsed()));
            if drive::output(&out) != CODEMODE_OUT || out.host_calls != CODEMODE_CALLS {
                return Err(format!("codemode {name}: {out:?}"));
            }
        }
        let host_s = stats(host);
        let py_s = stats(py);
        let per_call =
            host_s["median"].as_f64().unwrap_or(0.0) - py_s["median"].as_f64().unwrap_or(0.0);
        let cell_s = stats(cells);
        emit(&json!({"section": "hostcall", "mode": name,
            "sync_loop_us_per_iteration": host_s, "python_call_loop_us_per_iteration": py_s,
            "host_call_round_trip_us": per_call,
            "codemode_cell_us": cell_s,
            "codemode_us_per_host_call": cell_s["median"].as_f64().unwrap_or(0.0) / CODEMODE_CALLS as f64}));
    }
    Ok(())
}

// ----------------------------------------------------------------- bench

fn bench() -> Result<(), String> {
    let programs = drive::bench_programs(&crate::root());
    if programs.is_empty() {
        return Err("no bench programs".into());
    }
    let reps: usize = std::env::var("ISO_BENCH_REPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    for name in modes_for("bench") {
        let mode = Mode::parse(name)?;
        for (program, source, expected) in &programs {
            let mut xs = Vec::new();
            for i in 0..=reps {
                let mut b = mode.backend()?;
                drive::new_session(b.as_mut(), MEMORY)?;
                let t = Instant::now();
                let out = drive::run(b.as_mut(), source, Drive::default())?;
                let elapsed = us(t.elapsed());
                if &drive::output(&out) != expected {
                    return Err(format!("bench {name} {program}: {}", drive::output(&out)));
                }
                if i > 0 {
                    xs.push(elapsed / 1000.0);
                }
            }
            emit(&json!({"section": "bench", "mode": name, "program": program, "ms": stats(xs)}));
        }
    }
    Ok(())
}

// -------------------------------------------------------------- snapshot

/// The parked states: source and the host call to stop at.
pub fn states() -> Vec<(&'static str, String, u64)> {
    let dicts = |n: u32| {
        format!(
            "data = [{{'id': i, 'name': f'n{{i}}', 'tags': [i, i + 1]}} for i in range({n})]\nprint(len(data), tool(7))\n"
        )
    };
    vec![
        ("codemode@call100", CODEMODE.to_owned(), 100),
        ("dicts10k@call", dicts(10_000), 1),
        ("dicts100k@call", dicts(100_000), 1),
    ]
}

/// A backend stopped at `stop`'s host call of `source`.
pub fn parked(mode: &Mode, source: &str, stop: u64) -> Result<Box<dyn Backend>, String> {
    let mut b = mode.backend()?;
    drive::new_session(b.as_mut(), MEMORY)?;
    let out = drive::run(
        b.as_mut(),
        source,
        Drive {
            stop_at_call: Some(stop),
            ..Drive::default()
        },
    )?;
    if !matches!(out.response, Response::HostCall { .. }) {
        return Err(format!("not parked at a host call: {out:?}"));
    }
    Ok(b)
}

/// Answers the pending host call and runs the cell to its end.
pub fn finish(b: &mut dyn Backend, value: Json) -> Result<String, String> {
    let first = b
        .call(
            &Request::Resume {
                resume: ResumeMsg::Value { value },
                fuel: None,
            },
            &[],
        )?
        .0;
    Ok(drive::output(&drive::drive(b, first, Drive::default())?))
}

fn snapshot() -> Result<(), String> {
    for name in modes_for("snapshot") {
        let mode = Mode::parse(name)?;
        for (state, source, stop) in states() {
            let (mut save, mut restore, mut size) = (Vec::new(), Vec::new(), 0);
            for i in 0..=5 {
                let mut b = parked(&mode, &source, stop)?;
                let t = Instant::now();
                let (resp, bytes) = b.call(&Request::Snapshot, &[])?;
                let saved = t.elapsed();
                if !matches!(resp, Response::Snapshot { .. }) {
                    return Err(format!("snapshot {name}: {resp:?}"));
                }
                size = bytes.len();
                drop(b);
                let mut r = mode.backend()?;
                let t = Instant::now();
                let (resp, _) = r.call(&Request::Restore { memory: MEMORY }, &bytes)?;
                let restored = t.elapsed();
                if resp != Response::Ok {
                    return Err(format!("restore {name}: {resp:?}"));
                }
                let answer = if stop == 100 {
                    json!("{\"i\": 99}")
                } else {
                    json!(7)
                };
                let out = finish(r.as_mut(), answer)?;
                if !(out == CODEMODE_OUT || out.ends_with(" 7\n")) {
                    return Err(format!("resume after restore {name} {state}: {out}"));
                }
                if i > 0 {
                    save.push(us(saved));
                    restore.push(us(restored));
                }
            }
            emit(
                &json!({"section": "snapshot", "mode": name, "state": state, "bytes": size,
                "snapshot_us": stats(save), "restore_us": stats(restore)}),
            );
        }
    }
    Ok(())
}

/// Snapshots across modes: saved at a host call in one, restored and
/// finished in each.
fn cross() -> Result<(), String> {
    let names = modes_for("cross");
    let modes: Vec<Mode> = names
        .iter()
        .map(|n| Mode::parse(n))
        .collect::<Result<_, _>>()?;
    for (from, source) in names.iter().zip(&modes) {
        let mut b = parked(source, CODEMODE, 100)?;
        let (_, bytes) = b.call(&Request::Snapshot, &[])?;
        for (to, target) in names.iter().zip(&modes) {
            let mut r = target.backend()?;
            let restored = r.call(&Request::Restore { memory: MEMORY }, &bytes)?.0;
            let out = if restored == Response::Ok {
                finish(r.as_mut(), json!("{\"i\": 99}"))?
            } else {
                format!("{restored:?}")
            };
            emit(
                &json!({"section": "cross", "from": from, "to": to, "bytes": bytes.len(),
                "result": if out == CODEMODE_OUT { "resumed, output identical" } else { out.as_str() }}),
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- memory

fn memory() -> Result<(), String> {
    const N: usize = 100;
    let active = format!(
        "data = [{{'id': i, 'name': f'n{{i}}', 'tags': [i, i + 1]}} for i in range(1000)]\n{CODEMODE}"
    );
    for name in modes_for("memory") {
        let mode = Mode::parse(name)?;
        for (kind, source, stop) in [
            ("idle", "x = 1".to_owned(), None),
            ("active", active.clone(), Some(100)),
        ] {
            // Warm the allocator and the mode's shared state first.
            drop(make(&mode, &source, stop)?);
            let (rss0, _) = self_rss_hwm_kib();
            let mut sessions = Vec::new();
            for _ in 0..N {
                sessions.push(make(&mode, &source, stop)?);
            }
            let (rss1, _) = self_rss_hwm_kib();
            let host_kib = (rss1.saturating_sub(rss0)) as f64 / N as f64;
            let mut record = json!({"section": "memory", "mode": name, "state": kind, "sessions": N,
                "host_rss_delta_kib_per_session": host_kib});
            #[cfg(feature = "wasm")]
            if let Mode::Wasm(_) = mode {
                record["linear_memory_kib_per_session"] = json!(0);
            }
            if name == "child" {
                let mut rss = Vec::new();
                let mut pss = Vec::new();
                for s in &sessions {
                    if let Some(pid) = s.1 {
                        let (r, p) = rss_pss_kib(pid);
                        rss.push(r as f64);
                        pss.push(p as f64);
                    }
                }
                record["worker_rss_kib"] = stats(rss);
                record["worker_pss_kib"] = stats(pss);
            }
            #[cfg(feature = "wasm")]
            if let Some(kib) = sessions.first().and_then(|s| s.2) {
                record["linear_memory_kib_per_session"] = json!(kib);
            }
            emit(&record);
        }
    }
    Ok(())
}

type Made = (Box<dyn Backend>, Option<u32>, Option<u64>);

/// A session in `mode` that ran `source` (to completion, or stopped at a
/// host call), with the worker's pid and the instance's linear memory.
fn make(mode: &Mode, source: &str, stop: Option<u64>) -> Result<Made, String> {
    match mode {
        Mode::Child(config) => {
            let mut w = ChildWorker::spawn(config)?;
            drive::new_session(&mut w, MEMORY)?;
            drive::run(
                &mut w,
                source,
                Drive {
                    stop_at_call: stop,
                    ..Drive::default()
                },
            )?;
            let pid = w.pid();
            Ok((Box::new(w), Some(pid), None))
        }
        #[cfg(feature = "wasm")]
        Mode::Wasm(host) => {
            let mut w = host.instantiate()?;
            drive::new_session(&mut w, MEMORY)?;
            drive::run(
                &mut w,
                source,
                Drive {
                    stop_at_call: stop,
                    ..Drive::default()
                },
            )?;
            let kib = (w.memory_bytes() / 1024) as u64;
            Ok((Box::new(w), None, Some(kib)))
        }
        #[cfg(feature = "wasm")]
        Mode::Abi(rt) => {
            let mut w = crate::backend::Abi::new(std::sync::Arc::clone(rt));
            drive::new_session(&mut w, MEMORY)?;
            drive::run(
                &mut w,
                source,
                Drive {
                    stop_at_call: stop,
                    ..Drive::default()
                },
            )?;
            let kib = (w.memory_bytes() / 1024) as u64;
            Ok((Box::new(w), None, Some(kib)))
        }
        Mode::Native => {
            let mut b = mode.backend()?;
            drive::new_session(b.as_mut(), MEMORY)?;
            drive::run(
                b.as_mut(),
                source,
                Drive {
                    stop_at_call: stop,
                    ..Drive::default()
                },
            )?;
            Ok((b, None, None))
        }
    }
}

// ----------------------------------------------------------------- scale

fn scale() -> Result<(), String> {
    let threads_max = thread::available_parallelism()
        .map_or(16, usize::from)
        .min(16);
    for name in modes_for("scale") {
        let mode = Arc::new(Mode::parse(name)?);
        for n in [1usize, 16, 64, 256, 1024] {
            let threads = n.min(threads_max);
            let cells_per_session = (256 / n).max(1);
            // Build every session first (not timed), one bucket per thread.
            let mut buckets: Vec<Vec<Made>> = (0..threads).map(|_| Vec::new()).collect();
            for i in 0..n {
                buckets[i % threads].push(make(&mode, "x = 1", None)?);
            }
            let child_rss = |buckets: &Vec<Vec<Made>>| -> u64 {
                buckets
                    .iter()
                    .flatten()
                    .filter_map(|s| s.1)
                    .map(|pid| rss_pss_kib(pid).1)
                    .sum()
            };
            let resident_pss = child_rss(&buckets);
            let latencies = Arc::new(Mutex::new(Vec::new()));
            let errors = Arc::new(Mutex::new(Vec::new()));
            let start = Instant::now();
            let handles: Vec<_> = buckets
                .into_iter()
                .map(|mut bucket| {
                    let latencies = Arc::clone(&latencies);
                    let errors = Arc::clone(&errors);
                    thread::Builder::new().stack_size(16 << 20).spawn(move || {
                        for _ in 0..cells_per_session {
                            for s in &mut bucket {
                                let t = Instant::now();
                                match drive::run(s.0.as_mut(), CODEMODE, Drive::default()) {
                                    Ok(out) if drive::output(&out) == CODEMODE_OUT => {
                                        if let Ok(mut l) = latencies.lock() {
                                            l.push(us(t.elapsed()));
                                        }
                                    }
                                    other => {
                                        if let Ok(mut e) = errors.lock() {
                                            e.push(format!("{other:?}"));
                                        }
                                    }
                                }
                            }
                        }
                        bucket
                    })
                })
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?;
            let mut buckets = Vec::new();
            for h in handles {
                buckets.push(h.join().map_err(|_| "scale thread panicked")?);
            }
            let wall = start.elapsed();
            let (rss, _) = self_rss_hwm_kib();
            let errs = errors.lock().map(|e| e.len()).unwrap_or(0);
            let lat = latencies.lock().map(|l| l.clone()).unwrap_or_default();
            let cells = lat.len();
            emit(
                &json!({"section": "scale", "mode": name, "sessions": n, "threads": threads,
                "cells": cells, "errors": errs, "wall_ms": wall.as_secs_f64() * 1e3,
                "cells_per_s": cells as f64 / wall.as_secs_f64(),
                "cell_latency_us": if lat.is_empty() { Json::Null } else { stats(lat) },
                "host_rss_kib": rss, "workers_pss_kib_sum": child_rss(&buckets) .max(resident_pss)}),
            );
            drop(buckets);
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ soak

/// One agent's cell: sync calls, one awaited future, `gather` batches, and
/// state kept across cells (a bounded history), as codemode agents write.
/// 201 host calls (207 host round trips with the `Waiting` batches).
const SOAK: &str = r#"import json, asyncio
try:
    hist
except NameError:
    hist = []
total = 0
for i in range(100):
    r = json.loads(tool({"i": i, "q": "item"}))
    total += r["i"]
async def main():
    one = json.loads(await atool({"i": 7}))["i"]
    acc = 0
    for b in range(5):
        rs = await asyncio.gather(*[atool({"i": b * 20 + j}) for j in range(20)])
        acc += sum(json.loads(r)["i"] for r in rs)
    return one + acc
res = asyncio.run(main())
hist.append({"total": total, "res": res, "n": len(hist)})
hist = hist[-50:]
print(total, res)
"#;
const SOAK_OUT: &str = "4950 4957\n";

/// A latency histogram in µs with 1% wide logarithmic buckets from 0.01 µs,
/// so the soak's own bookkeeping stays a few KiB per thread.
struct Hist(Vec<u64>);

impl Hist {
    const BUCKETS: usize = 2400;
    fn new() -> Hist {
        Hist(vec![0; Self::BUCKETS])
    }
    fn add(&mut self, us: f64) {
        let i = ((us.max(0.01) / 0.01).ln() / 1.01f64.ln()) as usize;
        self.0[i.min(Self::BUCKETS - 1)] += 1;
    }
    fn merge(&mut self, other: &Hist) {
        for (a, b) in self.0.iter_mut().zip(&other.0) {
            *a += b;
        }
    }
    fn count(&self) -> u64 {
        self.0.iter().sum()
    }
    fn json(&self) -> Json {
        let n = self.count();
        if n == 0 {
            return Json::Null;
        }
        let at = |q: f64| {
            let target = ((n - 1) as f64 * q).round() as u64;
            let mut seen = 0;
            for (i, c) in self.0.iter().enumerate() {
                seen += c;
                if seen > target {
                    return 0.01 * 1.01f64.powi(i as i32);
                }
            }
            f64::NAN
        };
        json!({"n": n, "median": at(0.5), "p90": at(0.9), "p99": at(0.99), "p999": at(0.999), "max_bucket": at(1.0)})
    }
}

/// `ISO_SOAK_SESSIONS` (default 40) sessions, each on its own thread, run
/// [`SOAK`] for `ISO_SOAK_SECS` (default 300) seconds, sleeping
/// `ISO_SOAK_THINK_MS` (default 0: back to back) between cells. Every host
/// round trip (answer sent → next stop back) and every cell is timed; host
/// RSS is sampled every second.
fn soak() -> Result<(), String> {
    let env = |k: &str, d: u64| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let n = env("ISO_SOAK_SESSIONS", 40) as usize;
    let secs = env("ISO_SOAK_SECS", 300);
    let think = Duration::from_millis(env("ISO_SOAK_THINK_MS", 0));
    let modes: Vec<String> = match std::env::var("ISO_MODES") {
        Ok(m) if !m.is_empty() => m.split(',').map(str::to_owned).collect(),
        _ => vec!["native".to_owned(), "abi".to_owned()],
    };
    for name in &modes {
        let mode = Arc::new(Mode::parse(name)?);
        drop(make(&mode, SOAK, None)?);
        let (rss0, _) = self_rss_hwm_kib();
        let mut sessions = Vec::new();
        for _ in 0..n {
            sessions.push(make(&mode, "x = 1", None)?);
        }
        let (rss_idle, _) = self_rss_hwm_kib();
        let deadline = Instant::now() + Duration::from_secs(secs);
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sampler = {
            let done = Arc::clone(&done);
            thread::spawn(move || {
                let mut samples = Vec::new();
                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                    samples.push(self_rss_hwm_kib().0);
                    thread::sleep(Duration::from_secs(1));
                }
                samples
            })
        };
        let start = Instant::now();
        let handles: Vec<_> = sessions
            .into_iter()
            .map(|mut s| {
                thread::Builder::new().stack_size(8 << 20).spawn(move || {
                    let (mut calls, mut cells, mut errors) = (Hist::new(), Hist::new(), Vec::new());
                    let mut errs = 0u64;
                    while Instant::now() < deadline {
                        let t = Instant::now();
                        match soak_cell(s.0.as_mut(), &mut calls) {
                            Ok(out) if out == SOAK_OUT => cells.add(us(t.elapsed())),
                            Ok(out) => {
                                errs += 1;
                                if errors.len() < 3 {
                                    errors.push(out);
                                }
                            }
                            Err(e) => {
                                errs += 1;
                                if errors.len() < 3 {
                                    errors.push(e);
                                }
                            }
                        }
                        if !think.is_zero() {
                            thread::sleep(think);
                        }
                    }
                    (s, calls, cells, errs, errors)
                })
            })
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let mut results = Vec::new();
        for h in handles {
            results.push(h.join().map_err(|_| "soak thread panicked")?);
        }
        let wall = start.elapsed();
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        let samples = sampler.join().map_err(|_| "sampler panicked")?;
        let (rss_end, hwm) = self_rss_hwm_kib();
        let (mut calls, mut cells, mut errs, mut first) = (Hist::new(), Hist::new(), 0, Vec::new());
        let mut linear = Vec::new();
        for (s, c, l, e, f) in &results {
            calls.merge(c);
            cells.merge(l);
            errs += e;
            first.extend(f.iter().take(3 - first.len().min(3)).cloned());
            if let Some(kib) = s.0.linear_kib() {
                linear.push(kib as f64);
            }
        }
        drop(results);
        let peak = samples.iter().copied().max().unwrap_or(rss_end);
        let (ncells, ncalls) = (cells.count(), calls.count());
        let per = |kib: u64| (kib.saturating_sub(rss0)) as f64 / n as f64;
        emit(
            &json!({"section": "soak", "mode": name, "sessions": n, "think_ms": think.as_millis() as u64,
            "secs": wall.as_secs_f64(), "cells": ncells, "host_round_trips": ncalls, "errors": errs,
            "first_errors": first,
            "cells_per_s": ncells as f64 / wall.as_secs_f64(),
            "host_round_trips_per_s": ncalls as f64 / wall.as_secs_f64(),
            "host_round_trip_us": calls.json(), "cell_us": cells.json(),
            "host_rss_kib": {"before_sessions": rss0, "idle_sessions": rss_idle,
                "peak_sampled": peak, "end": rss_end, "process_hwm": hwm,
                "after_60s": samples.get(60).copied(), "last_sample": samples.last().copied()},
            "rss_per_session_kib": {"idle": per(rss_idle), "loaded_peak": per(peak), "loaded_end": per(rss_end)},
            "linear_memory_kib": if linear.is_empty() { Json::Null } else { stats(linear) }}),
        );
    }
    Ok(())
}

/// Runs [`SOAK`] once, timing every host round trip into `calls`.
fn soak_cell(b: &mut dyn Backend, calls: &mut Hist) -> Result<String, String> {
    let mut pending = std::collections::HashMap::new();
    let mut response = b
        .call(
            &Request::Run {
                source: SOAK.to_owned(),
                fuel: None,
            },
            &[],
        )?
        .0;
    loop {
        let resume = match &response {
            Response::HostCall { call, name, args } => {
                if name == "atool" {
                    pending.insert(*call, drive::answer(args));
                    ResumeMsg::Future
                } else {
                    ResumeMsg::Value {
                        value: drive::answer(args),
                    }
                }
            }
            Response::Waiting { calls: ids } => ResumeMsg::Batch {
                results: ids
                    .iter()
                    .map(|c| (*c, pending.remove(c).unwrap_or(Json::Null)))
                    .collect(),
            },
            Response::Complete { output, .. } => return Ok(output.clone()),
            other => return Ok(format!("{other:?}")),
        };
        let t = Instant::now();
        response = b.call(&Request::Resume { resume, fuel: None }, &[])?.0;
        calls.add(us(t.elapsed()));
    }
}
