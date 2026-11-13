//! Per-session memory and first-import time of a guest: the bundle guest
//! against the default guest (bench/evidence/host-bundle/README.md at
//! da1c21b5).
//!
//! cargo run --release --example bundle_mem -- GUEST.wasm [SESSIONS]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    reason = "measurement"
)]

use std::{path::PathBuf, time::Instant};

use micro_monty::Progress;
use mm_wasm_host::{Runtime, RuntimeConfig, SessionOptions};

fn rss() -> u64 {
    let s = std::fs::read_to_string("/proc/self/statm").unwrap();
    s.split_whitespace().nth(1).unwrap().parse::<u64>().unwrap() * 4096
}

fn run(s: &mut mm_wasm_host::Session, code: &str) {
    match s.start(code, "cell.py").unwrap() {
        Progress::Complete(_) => {}
        p => panic!("{code}: {p:?}"),
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let guest = PathBuf::from(args.next().unwrap());
    let n: usize = args.next().map_or(100, |a| a.parse().unwrap());
    let rt = Runtime::new(&guest, RuntimeConfig::default()).unwrap();
    let o = SessionOptions::default();
    println!("load {:.1} ms", rt.load_time.as_secs_f64() * 1e3);

    // Warm the process (allocator, code pages) with one session.
    run(&mut rt.session(&o).unwrap(), "import json, re, collections");

    let before = rss();
    let mut idle: Vec<_> = (0..n).map(|_| rt.session(&o).unwrap()).collect();
    let after_idle = rss();
    let lin_idle = idle[0].memory_bytes();
    for s in &mut idle {
        run(s, "x = 1 + 2\nprint(x)");
    }
    let after_cell = rss();
    let lin_cell = idle[0].memory_bytes();
    for s in &mut idle {
        run(s, "import json, re, collections\ny = json.dumps({'a': [1, 2]})");
    }
    let after_imports = rss();
    let lin_imports = idle[0].memory_bytes();
    let per = |a: u64, b: u64| (a.saturating_sub(b)) as f64 / n as f64 / 1024.0;
    println!(
        "sessions {n}: RSS/session idle {:.0} KiB, after a cell {:.0} KiB, after json+re+collections {:.0} KiB",
        per(after_idle, before),
        per(after_cell, before),
        per(after_imports, before)
    );
    println!(
        "linear memory: idle {} KiB, after a cell {} KiB, after imports {} KiB",
        lin_idle / 1024,
        lin_cell / 1024,
        lin_imports / 1024
    );
    drop(idle);

    // Session creation and first-import times in fresh sessions.
    let reps = 41;
    let mut new = Vec::new();
    let mut first = Vec::new();
    for _ in 0..reps {
        let t = Instant::now();
        let mut s = rt.session(&o).unwrap();
        new.push(t.elapsed().as_secs_f64() * 1e6);
        let t = Instant::now();
        run(&mut s, "x = 1");
        first.push(t.elapsed().as_secs_f64() * 1e6);
    }
    println!(
        "session new {:.0} us, first cell {:.0} us (medians of {reps})",
        median(new),
        median(first)
    );
    for m in ["json", "re", "collections", "datetime"] {
        let mut v = Vec::new();
        for _ in 0..reps {
            let mut s = rt.session(&o).unwrap();
            run(&mut s, "x = 1");
            let t = Instant::now();
            run(&mut s, &format!("import {m}"));
            v.push(t.elapsed().as_secs_f64() * 1e6);
        }
        println!("first import {m}: {:.0} us", median(v));
    }
}
