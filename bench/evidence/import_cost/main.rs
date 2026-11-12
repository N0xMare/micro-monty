//! `imp`: what importing bundled modules costs a session
//! (docs/research/import-cost.md).
//!
//! ```text
//! imp ir MODS N            N times: Session::new, then `import MODS` (callgrind differencing)
//! imp time MODS REPS       wall ns: the first operation, then median and min of REPS
//! imp rss MODS N           RSS before and after holding N sessions that imported MODS
//! imp stmts CHUNKS REPS    a module's top-level statements (a JSON list of sources) run
//!                          one cell each in a fresh session; median wall ns per chunk
//! ```
//!
//! MODS is a comma-separated module list (`json,re`); `-` is no import;
//! `@FILE` runs FILE as the cell instead (a module used, not only imported).
//! The engine has the facade's default decoded-module cache, so sessions
//! after the first reuse its decoded modules; `IMP_CACHE=0` turns it off.
//! The import cell deletes the names it bound, as session-templates' does.
#![forbid(unsafe_code)]

use micro_monty::{Engine, EngineConfig, Progress, Session};
use serde_json::json;
use std::time::Instant;

type Res<T> = Result<T, String>;

fn zones(s: &mut Session) {
    s.set_zone_source(Some(Box::new(|key: &str| {
        if key.starts_with('/') || key.split('/').any(|p| p == ".." || p.is_empty()) {
            return None;
        }
        std::fs::read(std::path::Path::new("/usr/share/zoneinfo").join(key))
            .ok()
            .filter(|b| b.starts_with(b"TZif"))
    })));
}

fn cell(mods: &str) -> String {
    if mods == "-" || mods.is_empty() {
        return String::new();
    }
    if let Some(path) = mods.strip_prefix('@') {
        return std::fs::read_to_string(path).unwrap_or_default();
    }
    let m: Vec<&str> = mods.split(',').collect();
    let mut top: Vec<&str> = m.iter().map(|n| n.split('.').next().unwrap_or(n)).collect();
    top.dedup();
    format!("import {}\ndel {}\n", m.join(", "), top.join(", "))
}

fn run(engine: &Engine, s: &mut Session, src: &str) -> Res<()> {
    let code = engine.compile(src, "<cell>").map_err(|e| e.rendered)?;
    match s.start(&code) {
        Progress::Complete(_) => Ok(()),
        other => Err(format!("{src:?}: {other:?}")),
    }
}

fn fresh(engine: &Engine, src: &str) -> Res<Session> {
    let mut s = Session::new(engine).map_err(|e| e.traceback)?;
    zones(&mut s);
    // A fixed clock, as the codemode runner's.
    s.set_wall_clock(Some(Box::new(|| 1_773_576_000.0)));
    if !src.is_empty() {
        run(engine, &mut s, src)?;
    }
    Ok(s)
}

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("VmRSS:"))
                .and_then(|v| v.split_whitespace().next()?.parse().ok())
        })
        .unwrap_or(0)
}

fn median(mut v: Vec<u128>) -> u128 {
    v.sort_unstable();
    v[v.len() / 2]
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let arg = |i: usize| a.get(i).map(String::as_str).unwrap_or("");
    let num = |i: usize| {
        arg(i)
            .parse::<usize>()
            .map_err(|_| format!("argument {i}: a number"))
    };
    // `IMP_CACHE=0`: no decoded-module cache (each session decodes the
    // modules it imports, as one session per process does).
    let engine = if std::env::var("IMP_CACHE").as_deref() == Ok("0") {
        Engine::with_module_cache(EngineConfig::default(), 0)
    } else {
        Engine::new(EngineConfig::default())
    };
    let result: Res<()> = (|| match arg(1) {
        "ir" => {
            let src = cell(arg(2));
            for _ in 0..num(3)? {
                drop(fresh(&engine, &src)?);
            }
            Ok(())
        }
        "time" => {
            let src = cell(arg(2));
            let mut times = Vec::new();
            for _ in 0..=num(3)? {
                let t = Instant::now();
                let s = fresh(&engine, &src)?;
                times.push(t.elapsed().as_nanos());
                drop(s);
            }
            let first = times.remove(0);
            let min = *times.iter().min().unwrap_or(&0);
            println!(
                "{}",
                json!({"first_ns": first, "median_ns": median(times), "min_ns": min})
            );
            Ok(())
        }
        "rss" => {
            let src = cell(arg(2));
            drop(fresh(&engine, &src)?);
            let before = rss_kib();
            let held: Vec<Session> = (0..num(3)?)
                .map(|_| fresh(&engine, &src))
                .collect::<Res<_>>()?;
            let after = rss_kib();
            println!(
                "{}",
                json!({"sessions": held.len(), "before_kib": before, "after_kib": after})
            );
            Ok(())
        }
        "stmts" => {
            // One cell: the statements with a clock reading between each
            // two, from a monotonic clock the session reads as `_mm.clock`.
            let text = std::fs::read_to_string(arg(2)).map_err(|e| e.to_string())?;
            let chunks: Vec<String> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let mut src = String::from("import _mm as __mm\n__T = [__mm.clock()]\n");
            for c in &chunks {
                src.push_str(c);
                src.push_str("\n__T.append(__mm.clock())\n");
            }
            src.push_str("__T\n");
            let code = engine.compile(&src, "<stmts>").map_err(|e| e.rendered)?;
            let mut times = vec![Vec::new(); chunks.len()];
            for _ in 0..num(3)? {
                let mut s = Session::new(&engine).map_err(|e| e.traceback)?;
                zones(&mut s);
                s.set_last_expr_value(true);
                let t0 = Instant::now();
                s.set_wall_clock(Some(Box::new(move || t0.elapsed().as_secs_f64())));
                let Progress::Complete(Some(c)) = s.start(&code) else {
                    return Err("stmts did not complete".into());
                };
                let v: Vec<f64> = serde_json::from_str(&c.repr).map_err(|e| e.to_string())?;
                for (i, w) in v.windows(2).enumerate() {
                    times[i].push(((w[1] - w[0]) * 1e9) as u128);
                }
            }
            let out: Vec<u128> = times.into_iter().map(median).collect();
            println!("{}", json!(out));
            Ok(())
        }
        _ => Err("usage: imp ir|time|rss|stmts ...".into()),
    })();
    if let Err(e) = result {
        eprintln!("imp: {e}");
        std::process::exit(1);
    }
}
