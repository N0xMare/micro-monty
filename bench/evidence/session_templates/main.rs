//! `tpl`: session templates (a snapshot taken after the common imports,
//! restored per session) against `Session::new` plus the imports, on the
//! unchanged facade. docs/research/session-templates.md.
//!
//! ```text
//! tpl image SET OUT                      the template image for SET
//! tpl ir fresh|restore SET N [IMAGE]      do the operation N times (callgrind differencing)
//! tpl time fresh|restore SET REPS IMAGE  wall ns: the first operation, then median and min of REPS
//! tpl rss fresh|restore SET N IMAGE      RSS before and after holding N sessions
//! tpl own FILE SET IMAGE                 a case after fresh imports and from the template (JSON)
//! tpl leak IMAGE                         sessions from one template share nothing
//! tpl settings                           which host-side settings an image keeps
//! tpl check IMAGE                        restore an image (build id check)
//! ```
//!
//! `TPL_MODULE_CACHE=BYTES` sets the engine's module cache limit
//! (`Engine::with_module_cache`; 0 turns it off); `tpl rss` also reports
//! the bytes the cache holds.
//!
//! SET is `empty`, `json`, `codemode`, `codemode-tz` or `heavy`. A fresh
//! operation is `Session::new`, the zone source, then the import cell; a
//! restore is `Engine::restore` of the template image.
#![forbid(unsafe_code)]

use micro_monty::{
    Engine, EngineConfig, HintGroup, HostArgs, HostModule, Limits, Progress, Session,
    SessionTemplate, TextCut,
};
use serde_json::json;
use std::{
    sync::{Arc, Mutex, PoisonError},
    time::Instant,
};

type Res<T> = Result<T, String>;

/// The modules of SET.
fn modules(set: &str) -> Res<&'static [&'static str]> {
    Ok(match set {
        "empty" => &[],
        "json" => &["json"],
        "codemode" => &["json", "asyncio", "re", "collections", "datetime"],
        "codemode-tz" => &[
            "json",
            "asyncio",
            "re",
            "collections",
            "datetime",
            "zoneinfo",
            "calendar",
        ],
        "heavy" => &[
            "decimal",
            "statistics",
            "zoneinfo",
            "html.parser",
            "tomllib",
        ],
        _ => return Err(format!("unknown set {set}")),
    })
}

/// The import cell: loads the modules and leaves no names in the globals,
/// so a cell that forgets an import fails as in a fresh session.
fn imports(set: &str) -> Res<String> {
    let m = modules(set)?;
    if m.is_empty() {
        return Ok(String::new());
    }
    let mut top: Vec<&str> = m.iter().map(|n| n.split('.').next().unwrap_or(n)).collect();
    top.dedup();
    Ok(format!("import {}\ndel {}\n", m.join(", "), top.join(", ")))
}

/// The zone source `mm` and the codemode runner use.
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

fn run(engine: &Engine, s: &mut Session, src: &str) -> Res<Progress> {
    let code = engine.compile(src, "<cell>").map_err(|e| e.rendered)?;
    Ok(s.start(&code))
}

fn fresh(engine: &Engine, set: &str) -> Res<Session> {
    let mut s = Session::new(engine).map_err(|e| e.traceback)?;
    zones(&mut s);
    // `TPL_PINNED=clock,entropy`: a fixed clock and/or entropy, to find what makes two
    // images of one set differ.
    let pinned = std::env::var("TPL_PINNED").unwrap_or_default();
    if pinned.contains("clock") {
        s.set_wall_clock(Some(Box::new(|| 1_773_576_000.0)));
    }
    if pinned.contains("entropy") {
        s.set_entropy(Some(Box::new(|b: &mut [u8]| b.fill(7))));
    }
    let src = imports(set)?;
    if !src.is_empty() {
        match run(engine, &mut s, &src)? {
            Progress::Complete(_) => {}
            other => return Err(format!("imports: {other:?}")),
        }
    }
    Ok(s)
}

fn restore(engine: &Engine, image: &[u8]) -> Res<Session> {
    engine.restore(image).map_err(|e| e.to_string())
}

fn read(path: Option<&String>) -> Res<Vec<u8>> {
    let p = path.ok_or("IMAGE")?;
    std::fs::read(p).map_err(|e| format!("{p}: {e}"))
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

fn op(mode: &str, engine: &Engine, set: &str, image: &[u8]) -> Res<Session> {
    match mode {
        "fresh" => fresh(engine, set),
        "restore" => restore(engine, image),
        _ => Err(format!("unknown mode {mode}")),
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
    let engine = match std::env::var("TPL_MODULE_CACHE") {
        Ok(n) => Engine::with_module_cache(EngineConfig::default(), n.parse().unwrap_or(0)),
        Err(_) => Engine::new(EngineConfig::default()),
    };
    let result: Res<()> = (|| match arg(1) {
        "image" => {
            let mut s = fresh(&engine, arg(2))?;
            let image = s.snapshot().map_err(|e| e.to_string())?;
            std::fs::write(arg(3), &image).map_err(|e| e.to_string())?;
            println!("{} {} bytes", arg(2), image.len());
            Ok(())
        }
        "ir" => {
            let image = if arg(2) == "restore" {
                read(a.get(5))?
            } else {
                Vec::new()
            };
            for _ in 0..num(4)? {
                drop(op(arg(2), &engine, arg(3), &image)?);
            }
            Ok(())
        }
        "time" => {
            let image = read(a.get(5))?;
            let mut times = Vec::new();
            for _ in 0..=num(4)? {
                let t = Instant::now();
                let s = op(arg(2), &engine, arg(3), &image)?;
                times.push(t.elapsed().as_nanos());
                drop(s);
            }
            let first = times.remove(0);
            times.sort_unstable();
            println!(
                "{}",
                json!({"first_ns": first, "median_ns": times[times.len() / 2], "min_ns": times[0]})
            );
            Ok(())
        }
        "rss" => {
            let image = read(a.get(5))?;
            drop(op(arg(2), &engine, arg(3), &image)?);
            let before = rss_kib();
            let held: Vec<Session> = (0..num(4)?)
                .map(|_| op(arg(2), &engine, arg(3), &image))
                .collect::<Res<_>>()?;
            let after = rss_kib();
            println!(
                "{}",
                json!({"sessions": held.len(), "before_kib": before, "after_kib": after,
                       "module_cache_bytes": engine.module_cache_bytes()})
            );
            Ok(())
        }
        "own" => own(&engine, arg(2), arg(3), &read(a.get(4))?),
        "leak" => leak(&read(a.get(2))?),
        "settings" => settings(),
        "check" => {
            let image = read(a.get(2))?;
            let r = engine.restore(&image).map(drop);
            println!(
                "{}",
                json!({"restore": format!("{r:?}"), "image_build_id_matches":
                image.get(10..42) == Some(micro_monty::build_id().as_slice())})
            );
            Ok(())
        }
        _ => Err("usage: see the crate docs".into()),
    })();
    if let Err(e) = result {
        eprintln!("tpl: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Behaviour.

/// What a cell did: output and how it ended.
fn cell(
    engine: &Engine,
    s: &mut Session,
    src: &str,
    out: &Arc<Mutex<String>>,
) -> Res<serde_json::Value> {
    let sink = Arc::clone(out);
    s.set_output(Box::new(move |t: &str| {
        sink.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_str(t);
    }));
    zones(s);
    s.set_module_dunders(None, None).map_err(|e| e.traceback)?;
    s.set_fuel(Some(2_000_000_000));
    let end = match run(engine, s, src) {
        Err(rendered) => json!({"compile_error": rendered}),
        Ok(Progress::Complete(v)) => json!({"value": v.map(|v| v.repr)}),
        Ok(Progress::Error(e)) => json!({"error": e.traceback}),
        Ok(other) => json!({"stopped": format!("{other:?}")}),
    };
    let stdout = std::mem::take(&mut *out.lock().unwrap_or_else(PoisonError::into_inner));
    Ok(json!({"stdout": stdout, "end": end}))
}

/// An own case run after the set's imports in a fresh session, and in a
/// session restored from the set's template.
fn own(engine: &Engine, file: &str, set: &str, image: &[u8]) -> Res<()> {
    let src = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let out = Arc::new(Mutex::new(String::new()));
    let mut f = fresh(engine, set)?;
    let a = cell(engine, &mut f, &src, &out)?;
    let mut t = restore(engine, image)?;
    let b = cell(engine, &mut t, &src, &out)?;
    println!(
        "{}",
        json!({"case": file, "same": a == b, "fresh": a, "template": b})
    );
    Ok(())
}

fn value(engine: &Engine, s: &mut Session, src: &str) -> Res<String> {
    match run(engine, s, src)? {
        Progress::Complete(Some(c)) => Ok(c.repr),
        Progress::Complete(None) => Ok("<no value>".into()),
        Progress::Error(e) => Ok(format!("<{}: {}>", e.type_name, e.message)),
        other => Ok(format!("<{other:?}>")),
    }
}

const MUTATE: &str = "\
import json, collections, re, sys, random, datetime
json.dumps = None
collections.Counter.leaked = 1
sys.leaked = 1
re.compile('leaked(x)')
random.seed(12345)
LEAKED = [1] * 1000
datetime.timedelta.leaked = 2
";

const PROBE: &str = "\
import json, collections, sys, datetime
(json.dumps is None, hasattr(collections.Counter, 'leaked'), hasattr(sys, 'leaked'), \
'LEAKED' in globals(), hasattr(datetime.timedelta, 'leaked'))";

/// Sessions from one template, in one thread and in eight at once: a
/// mutation in one is invisible to every other.
fn leak(image: &[u8]) -> Res<()> {
    let engine = Engine::new(EngineConfig::default());
    let template =
        Arc::new(SessionTemplate::from_image(&engine, image.to_vec()).map_err(|e| e.to_string())?);
    let clean = "(False, False, False, False, False)";
    let mut a = template.instantiate().map_err(|e| e.to_string())?;
    let mut b = template.instantiate().map_err(|e| e.to_string())?;
    let before_b = value(&engine, &mut b, PROBE)?;
    run(&engine, &mut a, MUTATE)?;
    let in_a = value(&engine, &mut a, PROBE)?;
    let in_b = value(&engine, &mut b, PROBE)?;
    let mut c = template.instantiate().map_err(|e| e.to_string())?;
    let in_c = value(&engine, &mut c, PROBE)?;
    // Random state: a template captured after `import random` hands every
    // instance the same generator state.
    let rnd = |s: &mut Session| value(&engine, s, "import random; random.random()");
    let (r1, r2) = (rnd(&mut b)?, rnd(&mut c)?);
    let fresh_r: Vec<String> = (0..2)
        .map(|_| {
            let mut s = Session::new(&engine).map_err(|e| e.traceback)?;
            rnd(&mut s)
        })
        .collect::<Res<_>>()?;
    let urandom = |s: &mut Session| value(&engine, s, "import os; os.urandom(8).hex()");
    let mut d = template.instantiate().map_err(|e| e.to_string())?;
    let mut e = template.instantiate().map_err(|e| e.to_string())?;
    let (u1, u2) = (urandom(&mut d)?, urandom(&mut e)?);
    let reseed =
        |s: &mut Session| value(&engine, s, "import random; random.seed(); random.random()");
    let mut f = template.instantiate().map_err(|e| e.to_string())?;
    let mut g = template.instantiate().map_err(|e| e.to_string())?;
    let (s1, s2) = (reseed(&mut f)?, reseed(&mut g)?);
    let hash = |s: &mut Session| value(&engine, s, "hash('abc'), list({'a', 'b', 'c', 'd'})");
    let mut h1 = Session::new(&engine).map_err(|e| e.traceback)?;
    let mut h2 = Session::new(&engine).map_err(|e| e.traceback)?;
    let (hf1, hf2) = (hash(&mut h1)?, hash(&mut h2)?);
    let (ht1, ht2) = (hash(&mut f)?, hash(&mut g)?);
    // `time` keeps its import-time readings: the process-clock zero and
    // the monotonic floor. An instance whose wall clock is earlier than the
    // capture's sees `monotonic()` stand still at the capture's reading.
    let clocks = "import time; a = time.monotonic(); b = time.monotonic(); (time.time(), a, b, time.process_time())";
    let mut k = template.instantiate().map_err(|e| e.to_string())?;
    let clock_system = value(&engine, &mut k, clocks)?;
    let mut l = template.instantiate().map_err(|e| e.to_string())?;
    let t0 = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let t1 = std::sync::Arc::clone(&t0);
    l.set_wall_clock(Some(Box::new(move || {
        1_773_576_000.0 + t1.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as f64
    })));
    let clock_earlier = value(&engine, &mut l, clocks)?;
    let mut m = Session::new(&engine).map_err(|e| e.traceback)?;
    let t2 = std::sync::Arc::clone(&t0);
    m.set_wall_clock(Some(Box::new(move || {
        1_773_576_000.0 + t2.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as f64
    })));
    let clock_fresh = value(&engine, &mut m, clocks)?;
    let threads: Vec<_> = (0..8)
        .map(|i| {
            let t = Arc::clone(&template);
            std::thread::spawn(move || -> Res<bool> {
                let engine = Engine::new(EngineConfig::default());
                let mut ok = true;
                for _ in 0..25 {
                    let mut s = t.instantiate().map_err(|e| e.to_string())?;
                    ok &= value(&engine, &mut s, PROBE)? == clean;
                    run(&engine, &mut s, MUTATE)?;
                    run(&engine, &mut s, &format!("json.marker = {i}"))?;
                    ok &= value(&engine, &mut s, "json.marker")? == i.to_string();
                }
                Ok(ok)
            })
        })
        .collect();
    let mut all = true;
    for t in threads {
        all &= t.join().map_err(|_| "thread panicked")??;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "b_before_a_mutates": before_b, "a_after_mutating": in_a,
            "b_after_a_mutated": in_b, "c_instantiated_after_a_mutated": in_c,
            "clean_probe_value": clean,
            "random_random_in_two_template_instances": [r1, r2],
            "random_random_in_two_fresh_sessions": fresh_r,
            "os_urandom_in_two_template_instances": [u1, u2],
            "random_seed_then_random_in_two_template_instances": [s1, s2],
            "clocks_time_mono_mono_process__instance_system_clock": clock_system,
            "clocks__instance_with_an_earlier_ticking_clock": clock_earlier,
            "clocks__fresh_session_same_ticking_clock": clock_fresh,
            "str_hash_and_set_order_two_fresh_sessions": [hf1, hf2],
            "str_hash_and_set_order_two_template_instances": [ht1, ht2],
            "eight_threads_200_instances_clean_and_isolated": all,
        }))
        .map_err(|e| e.to_string())?
    );
    Ok(())
}

/// For each host-side setting: set it on the setup session, capture a
/// template, instantiate, and look at the instance.
fn settings() -> Res<()> {
    let engine = Engine::new(
        EngineConfig::new().with_limits(
            Limits::new()
                .with_memory(64 << 20)
                .with_recursion_depth(200)
                .with_fuel(Some(1_000_000_000)),
        ),
    );
    let sink = Arc::new(Mutex::new(String::new()));
    let configure = |sink: &Arc<Mutex<String>>| -> Res<Session> {
        let mut s = Session::new(&engine).map_err(|e| e.traceback)?;
        let sink2 = Arc::clone(sink);
        s.set_output(Box::new(move |t: &str| {
            sink2
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_str(t)
        }));
        s.set_module_dunders(Some("__main__"), Some("/setup.py"))
            .map_err(|e| e.traceback)?;
        s.set_host_args(HostArgs {
            max_depth: 1,
            ..HostArgs::default()
        });
        s.set_text_limit(40);
        s.set_text_cut(TextCut::HeadTail);
        s.set_hint(HintGroup::Network, Some("HOST-HINT-MARKER"))
            .map_err(|e| e.traceback)?;
        s.set_int_bits_text(true);
        s.set_last_expr_value(false);
        s.set_wall_clock(Some(Box::new(|| 42.0)));
        s.set_entropy(Some(Box::new(|b: &mut [u8]| b.fill(0))));
        let tokyo = std::fs::read("/usr/share/zoneinfo/Asia/Tokyo").map_err(|e| e.to_string())?;
        s.set_local_timezone(Some(tokyo));
        zones(&mut s);
        s.define(&HostModule::untyped("tools", &["ping"]))
            .map_err(|e| e.traceback)?;
        // Loads one zone through the source before the capture.
        run(
            &engine,
            &mut s,
            "import zoneinfo, random\nNY = zoneinfo.ZoneInfo('America/New_York')\nprint('setup')",
        )?;
        // Collect at every safepoint from here on (set after the setup cell,
        // which it would slow down a lot).
        s.set_gc_stress(true);
        Ok(s)
    };
    let mut s = configure(&sink)?;
    let mut control = configure(&Arc::new(Mutex::new(String::new())))?;
    // GC stress makes the control's cells very slow; it is not observable anyway.
    control.set_gc_stress(false);
    control.set_deadline(Some(Instant::now()));
    control.park_handle().request();
    let fuel_at_capture = s.fuel();
    let handle = s.park_handle();
    s.set_deadline(Some(Instant::now()));
    let template = SessionTemplate::capture(&engine, &mut s).map_err(|e| e.to_string())?;
    handle.request();
    // An engine with other limits loads the same image.
    let other = Engine::new(EngineConfig::default());
    let mut t = SessionTemplate::from_image(&other, template.image().to_vec())
        .map_err(|e| e.to_string())?
        .instantiate()
        .map_err(|e| e.to_string())?;
    let printed_before = sink.lock().unwrap_or_else(PoisonError::into_inner).len();
    let mut rows = serde_json::Map::new();
    let mut row = |k: &str, val: String| {
        rows.insert(k.to_owned(), json!(val));
    };
    row(
        "fuel at capture / instance fuel()",
        format!("{fuel_at_capture:?} / {:?}", t.fuel()),
    );
    row(
        "gc_stress",
        "not observable from a cell; restore sets it false [S mm-engine snapshot.rs]".into(),
    );
    let probes: [(&str, &str); 14] = [
        ("module dunders (__name__)", "print(__name__)"),
        ("last_expr_value off (1 + 1)", "1 + 1"),
        (
            "host_args max_depth 1 (value of [[1]], last_expr_value forced on)",
            "[[1]]",
        ),
        (
            "text_limit 40 + text_cut HeadTail (repr of 'x'*100)",
            "'x' * 100",
        ),
        ("int_bits_text (repr of 7**5000)", "7 ** 5000"),
        ("hint Network (import requests)", "import requests"),
        (
            "wall clock 42.0 (time.time())",
            "import time; print(time.time())",
        ),
        (
            "entropy zeros (os.urandom(4))",
            "import os; print(os.urandom(4).hex())",
        ),
        (
            "local timezone Asia/Tokyo (utcoffset)",
            "import datetime; print(datetime.datetime(2026, 1, 1).astimezone().utcoffset())",
        ),
        (
            "zone loaded before capture (America/New_York)",
            "import datetime; print(NY.utcoffset(datetime.datetime(2026, 1, 1)))",
        ),
        (
            "zone source (new key Europe/Paris)",
            "print(zoneinfo.ZoneInfo('Europe/Paris'))",
        ),
        (
            "recursion depth 200, restoring engine 1000 (sys.getrecursionlimit())",
            "import sys; print(sys.getrecursionlimit())",
        ),
        (
            "deadline in the past, park handle requested (a loop)",
            "n = 0\nfor i in range(300000): n += i\nprint(n)",
        ),
        (
            "host module defined in setup (tools.ping())",
            "import tools; tools.ping()",
        ),
    ];
    let mut values = Vec::new();
    for (i, (k, src)) in probes.iter().enumerate() {
        for (who, t) in [
            ("instance", &mut t),
            ("control (setting applied)", &mut control),
        ] {
            if i == 2 {
                t.set_last_expr_value(true);
            }
            let out = Arc::new(Mutex::new(String::new()));
            let o2 = Arc::clone(&out);
            t.set_output(Box::new(move |x: &str| {
                o2.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push_str(x)
            }));
            let end = match run(&other, t, src)? {
                Progress::Complete(c) => format!(
                    "complete, value {:?}",
                    c.map(|c| (c.repr, c.value.is_some()))
                ),
                Progress::Error(e) => {
                    let last = e
                        .traceback
                        .trim_end()
                        .lines()
                        .last()
                        .unwrap_or("")
                        .to_owned();
                    let hint = e.traceback.contains("HOST-HINT-MARKER");
                    format!("{last} (host hint in traceback: {hint})")
                }
                other => format!("{other:?}").chars().take(160).collect(),
            };
            let printed = out.lock().unwrap_or_else(PoisonError::into_inner).clone();
            values.push((
                format!("{k} [{who}]"),
                format!("{} | {}", printed.trim_end(), end),
            ));
        }
    }
    for (k, v) in values {
        row(&k, v);
    }
    let got = sink.lock().unwrap_or_else(PoisonError::into_inner).len() - printed_before;
    row(
        "setup's output callback (bytes it got from the instance)",
        got.to_string(),
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?
    );
    Ok(())
}
