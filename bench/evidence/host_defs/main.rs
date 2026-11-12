//! `hd`: what defining typed host modules costs a session, and what they
//! add to its snapshots (bench/evidence/results/2026-11-07-host-code).
//!
//! ```text
//! hd time TOOLS MODS REPS cold|warm   wall ns of Session::new + define: first, median, min
//! hd ir TOOLS MODS N cold|warm        N times Session::new + define (callgrind differencing)
//! hd rss TOOLS MODS N                 RSS before and after holding N defined sessions (warm)
//! hd snap TOOLS MODS DIR [stored]     DIR/base.mmsnap (a new session) and DIR/tools.mmsnap;
//!                                     `stored`: on an Engine that stores host code in images
//! hd restore REPS cold|warm IMAGE     wall ns of Engine::restore: first, median, min
//! hd call TOOLS MODS                  a defined session, snapshotted, restored, calls a tool
//! hd cache TOOLS MODS                 the Engine's cache bytes before and after defining
//! ```
//!
//! TOOLS typed functions spread over MODS modules (`tools0`, ...), each
//! with three annotated parameters (one keyword-only), defaults and a
//! docstring, half with `dict_as_kwargs`. "cold" makes each measured
//! session on a new Engine (its creation not timed); "warm" makes them on
//! one Engine that defined the modules once before.
#![forbid(unsafe_code)]

use micro_monty::{
    Engine, EngineConfig, HostModule, HostParam, HostSignature, HostValue, ParamKind, Progress,
    Resume, Session,
};
use serde_json::json;
use std::time::Instant;

type Res<T> = Result<T, String>;

fn tool(m: usize, i: usize) -> HostSignature {
    HostSignature {
        name: format!("tool_{i}"),
        params: vec![
            HostParam {
                name: "path".into(),
                annotation: "str".into(),
                ..HostParam::default()
            },
            HostParam {
                name: "limit".into(),
                annotation: "int | None".into(),
                default: Some(HostValue::None),
                ..HostParam::default()
            },
            HostParam {
                name: "verbose".into(),
                annotation: "bool".into(),
                default: Some(HostValue::Bool(false)),
                kind: ParamKind::KeywordOnly,
            },
        ],
        returns: "dict".into(),
        doc: format!("Tool {i} of module {m}: reads PATH and returns at most LIMIT entries."),
        dict_as_kwargs: i % 2 == 0,
    }
}

fn modules(tools: usize, mods: usize) -> Vec<HostModule> {
    let mods = mods.max(1);
    (0..mods)
        .map(|m| HostModule {
            name: format!("tools{m}"),
            functions: (0..tools)
                .filter(|i| i % mods == m)
                .map(|i| tool(m, i))
                .collect(),
            ..HostModule::default()
        })
        .collect()
}

fn defined(engine: &Engine, mods: &[HostModule]) -> Res<Session> {
    let mut s = Session::new(engine).map_err(|e| e.traceback)?;
    for m in mods {
        s.define(m).map_err(|e| e.to_string())?;
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
    v.get(v.len() / 2).copied().unwrap_or(0)
}

fn report(mut times: Vec<u128>) {
    let first = times.remove(0);
    let min = *times.iter().min().unwrap_or(&0);
    println!(
        "{}",
        json!({"first_ns": first, "median_ns": median(times), "min_ns": min})
    );
}

/// Engines for `n` measured operations: `n` new ones (cold), or one that
/// defined `mods` once (warm).
fn engines(mode: &str, n: usize, mods: &[HostModule]) -> Res<Vec<Engine>> {
    match mode {
        "cold" => Ok((0..n).map(|_| Engine::new(EngineConfig::default())).collect()),
        "warm" => {
            let e = Engine::new(EngineConfig::default());
            drop(defined(&e, mods)?);
            Ok(vec![e])
        }
        _ => Err("mode: cold or warm".into()),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let arg = |i: usize| a.get(i).map(String::as_str).unwrap_or("");
    let num = |i: usize| {
        arg(i)
            .parse::<usize>()
            .map_err(|_| format!("argument {i}: a number"))
    };
    let result: Res<()> = (|| match arg(1) {
        "time" | "ir" => {
            let mods = modules(num(2)?, num(3)?);
            let n = num(4)? + usize::from(arg(1) == "time");
            let es = engines(arg(5), n, &mods)?;
            let mut times = Vec::new();
            for i in 0..n {
                let e = &es[i % es.len()];
                let t = Instant::now();
                let s = defined(e, &mods)?;
                times.push(t.elapsed().as_nanos());
                drop(s);
            }
            if arg(1) == "time" {
                report(times);
            }
            Ok(())
        }
        "rss" => {
            let mods = modules(num(2)?, num(3)?);
            let e = Engine::new(EngineConfig::default());
            drop(defined(&e, &mods)?);
            let before = rss_kib();
            let held: Vec<Session> = (0..num(4)?)
                .map(|_| defined(&e, &mods))
                .collect::<Res<_>>()?;
            let after = rss_kib();
            println!(
                "{}",
                json!({"sessions": held.len(), "before_kib": before, "after_kib": after})
            );
            Ok(())
        }
        "snap" => {
            let mods = modules(num(2)?, num(3)?);
            let mut e = Engine::new(EngineConfig::default());
            if arg(5) == "stored" {
                e = e.with_host_code_in_snapshots(true);
            }
            let dir = std::path::Path::new(arg(4));
            let mut base = Session::new(&e).map_err(|err| err.traceback)?;
            let base = base.snapshot().map_err(|err| err.to_string())?;
            let tools = defined(&e, &mods)?
                .snapshot()
                .map_err(|err| err.to_string())?;
            std::fs::write(dir.join("base.mmsnap"), &base).map_err(|err| err.to_string())?;
            std::fs::write(dir.join("tools.mmsnap"), &tools).map_err(|err| err.to_string())?;
            println!("{}", json!({"base": base.len(), "tools": tools.len()}));
            Ok(())
        }
        "restore" => {
            let image = std::fs::read(arg(4)).map_err(|err| err.to_string())?;
            let n = num(2)? + 1;
            let es: Vec<Engine> = match arg(3) {
                "cold" => (0..n).map(|_| Engine::new(EngineConfig::default())).collect(),
                _ => {
                    let e = Engine::new(EngineConfig::default());
                    drop(e.restore(&image).map_err(|err| err.to_string())?);
                    vec![e]
                }
            };
            let mut times = Vec::new();
            for i in 0..n {
                let e = &es[i % es.len()];
                let t = Instant::now();
                let s = e.restore(&image).map_err(|err| err.to_string())?;
                times.push(t.elapsed().as_nanos());
                drop(s);
            }
            report(times);
            Ok(())
        }
        "call" => {
            let mods = modules(num(2)?, num(3)?);
            let e = Engine::new(EngineConfig::default());
            let image = defined(&e, &mods)?
                .snapshot()
                .map_err(|err| err.to_string())?;
            let e2 = Engine::new(EngineConfig::default());
            let mut s = e2.restore(&image).map_err(|err| err.to_string())?;
            let code = e2
                .compile(
                    "import tools0\ntools0.tool_0({'path': 'a', 'limit': 3})",
                    "<cell>",
                )
                .map_err(|err| err.rendered)?;
            let p = s.start(&code);
            let Progress::HostCall { name, args, .. } = &p else {
                return Err(format!("{p:?}"));
            };
            println!("{name} {args:?}");
            let p = s
                .resume(Resume::Value(HostValue::Int(7)))
                .map_err(|err| err.to_string())?;
            println!("{p:?}");
            Ok(())
        }
        "cache" => {
            let mods = modules(num(2)?, num(3)?);
            let e = Engine::new(EngineConfig::default());
            let before = e.module_cache_bytes();
            drop(defined(&e, &mods)?);
            println!("{}", json!({"before": before, "after": e.module_cache_bytes()}));
            Ok(())
        }
        _ => Err("usage: hd time|ir|rss|snap|restore|call|cache ...".into()),
    })();
    if let Err(e) = result {
        eprintln!("hd: {e}");
        std::process::exit(1);
    }
}
