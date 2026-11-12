//! Per-module import cost through `mm-wasm-host` (precompiled guest, one
//! session per instance): a new instance, `mm_session_new` and one import
//! cell, as `imp time` does natively (docs/research/import-cost.md).
//!
//! `wasm-imp GUEST.wasm REPS MODS...` prints one JSON line per MODS (a
//! comma-separated module list, `-` for none, `@FILE` for FILE as the
//! cell): wall ns of the first
//! operation and the median of REPS, and the guest's memory.

use mm_wasm_host::{Progress, Runtime, RuntimeConfig, Session, SessionOptions};
use serde_json::json;
use std::{path::Path, time::Instant};

fn fresh(rt: &Runtime, src: &str) -> Result<Session, String> {
    let mut s = rt
        .session(&SessionOptions::default())
        .map_err(|e| format!("{e:?}"))?;
    s.set_zone_source(Some(Box::new(|key: &str| {
        if key.starts_with('/') || key.split('/').any(|p| p == ".." || p.is_empty()) {
            return None;
        }
        std::fs::read(Path::new("/usr/share/zoneinfo").join(key))
            .ok()
            .filter(|b| b.starts_with(b"TZif"))
    })));
    if !src.is_empty() {
        match s.start(src, "<cell>").map_err(|e| format!("{e:?}"))? {
            Progress::Complete(_) => {}
            other => return Err(format!("imports: {other:?}")),
        }
    }
    Ok(s)
}

fn main() -> Result<(), String> {
    let a: Vec<String> = std::env::args().collect();
    let guest = Path::new(&a[1]);
    let reps: usize = a[2].parse().map_err(|_| "REPS")?;
    let rt = Runtime::new(guest, RuntimeConfig::default()).map_err(|e| format!("{e:?}"))?;
    for mods in &a[3..] {
        let src = if mods == "-" {
            String::new()
        } else if let Some(path) = mods.strip_prefix('@') {
            std::fs::read_to_string(path).map_err(|e| e.to_string())?
        } else {
            let m: Vec<&str> = mods.split(',').collect();
            let mut top: Vec<&str> = m.iter().map(|n| n.split('.').next().unwrap_or(n)).collect();
            top.dedup();
            format!("import {}\ndel {}\n", m.join(", "), top.join(", "))
        };
        let mut ns = Vec::new();
        let mut mem = 0;
        for _ in 0..=reps {
            let t = Instant::now();
            let s = fresh(&rt, &src)?;
            ns.push(t.elapsed().as_nanos());
            mem = s.memory_bytes();
        }
        let first = ns.remove(0);
        ns.sort_unstable();
        println!(
            "{}",
            json!({"mods": mods, "first_ns": first, "median_ns": ns[ns.len() / 2], "guest_memory": mem})
        );
    }
    Ok(())
}
