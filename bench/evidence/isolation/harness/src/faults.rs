//! One fault scenario per process: a bystander session stopped at a host
//! call, a victim session that meets the fault, then checks of the victim,
//! the bystander and a fresh session. Prints one JSON line. If the fault
//! takes the whole process down, the caller (`run.sh`) records the exit
//! status instead.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use iso_service::{Request, Response, ResumeMsg};
use serde_json::{Value as Json, json};

use crate::{
    Mode,
    backend::Backend,
    drive::{self, CODEMODE, CODEMODE_OUT, Drive},
    perf::{finish, parked},
};

const MEMORY: u64 = 256 << 20;

/// A cyclic dict compared with `==` (CPython raises `RecursionError`).
const CYCLIC_DICT: &str = "a = {}\na['k'] = a\nb = {}\nb['k'] = b\ntry:\n    print(a == b)\nexcept RecursionError as e:\n    print('RecursionError')\n";
/// Two 600-deep frozenset chains compared with `==` (CPython: `True`).
const FROZENSET_600: &str = "u, v = 'u', 'u'\nfor i in range(600):\n    u = frozenset([u, 2])\n    v = frozenset([v, 2])\nprint(u == v)\n";
const LOOP: &str = "while True:\n    pass\n";
/// One bigint multiply that runs for seconds inside a single native call.
const NATIVE: &str = "n = 7**4_000_000\nm = n*n\nprint(m.bit_length())\n";
const ARENA: &str = "x = []\nwhile True:\n    x.append('a' * 10000)\n";

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn describe(r: &Result<(Response, Vec<u8>), String>) -> Json {
    match r {
        Ok((Response::Complete { output, .. }, _)) => json!({"ok": "complete", "output": output}),
        Ok((
            Response::Error {
                type_name, message, ..
            },
            _,
        )) => {
            json!({"ok": "error", "type": type_name, "message": message.chars().take(120).collect::<String>()})
        }
        Ok((other, _)) => json!({"ok": format!("{other:?}").chars().take(160).collect::<String>()}),
        Err(e) => json!({"died": e}),
    }
}

pub fn main(args: &[String]) -> Result<(), String> {
    let mode = args.first().ok_or("fault MODE CASE")?.clone();
    let case = args.get(1).ok_or("fault MODE CASE")?.clone();
    let opts: Vec<(String, String)> = args[2..]
        .iter()
        .filter_map(|a| a.split_once('=').map(|(k, v)| (k.to_owned(), v.to_owned())))
        .collect();
    let host_stack_kib: usize = opts
        .iter()
        .find(|(k, _)| k == "host_stack_kib")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(8 << 10);
    let handle = thread::Builder::new()
        .stack_size(host_stack_kib << 10)
        .spawn(move || scenario(&mode, &case, &opts))
        .map_err(|e| e.to_string())?;
    let verdict = handle
        .join()
        .map_err(|_| "the fault thread panicked".to_owned())?
        // A scenario that cannot start (macOS refuses `RLIMIT_AS`, so the
        // worker never spawns) is the row's result, not a host death.
        .unwrap_or_else(|e| {
            json!({"mode": args[0], "case": args[1], "host": "survived",
                   "victim": {"died": format!("not started: {e}")}})
        });
    println!("{verdict}");
    Ok(())
}

fn opt<'a>(opts: &'a [(String, String)], key: &str) -> Option<&'a str> {
    opts.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn scenario(mode_name: &str, case: &str, opts: &[(String, String)]) -> Result<Json, String> {
    let mode = Mode::parse_with(mode_name, opts)?;
    let mut bystander = parked(&mode, CODEMODE, 100)?;
    let mut victim = mode.backend()?;
    let memory = opt(opts, "memory_mib")
        .and_then(|v| v.parse::<u64>().ok())
        .map_or(MEMORY, |m| m << 20);
    drive::new_session(victim.as_mut(), memory)?;
    let start = Instant::now();
    let mut record = json!({"mode": mode_name, "case": case, "opts": opts.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>()});
    let run = |b: &mut dyn Backend, source: &str, fuel: Option<u64>| {
        b.call(
            &Request::Run {
                source: source.to_owned(),
                fuel,
            },
            &[],
        )
    };
    match case {
        "stack-cyclic-dict" => {
            record["victim"] = describe(&run(victim.as_mut(), CYCLIC_DICT, None))
        }
        "stack-frozenset-600" => {
            record["victim"] = describe(&run(victim.as_mut(), FROZENSET_600, None))
        }
        "panic" => {
            victim.call(&Request::Inject { op: "start".into() }, &[])?;
            record["victim"] = describe(&run(victim.as_mut(), "print(1)", None));
        }
        "memory-arena" => record["victim"] = describe(&run(victim.as_mut(), ARENA, None)),
        "memory-source" => {
            let source = opt(opts, "src").unwrap_or("x = 1").replace("\\n", "\n");
            record["victim"] = describe(&run(victim.as_mut(), &source, None));
            record["host_hwm_kib"] = json!(crate::backend::self_rss_hwm_kib().1);
        }
        "runaway" => {
            let source = if opt(opts, "src") == Some("native") {
                NATIVE
            } else {
                LOOP
            };
            let guard = opt(opts, "guard").unwrap_or("fuel");
            let after = Duration::from_millis(
                opt(opts, "after_ms")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(200),
            );
            let fired = Arc::new(std::sync::Mutex::new(None::<Instant>));
            let result = match guard {
                // Engine fuel: the cell parks when its budget is spent.
                "fuel" => run(victim.as_mut(), source, Some(50_000_000)),
                // Park request (native, child) or epoch interrupt (wasm)
                // from another thread after `after`.
                "park" => {
                    let stop = victim.interrupter();
                    let fired2 = Arc::clone(&fired);
                    thread::spawn(move || {
                        thread::sleep(after);
                        if let Ok(mut f) = fired2.lock() {
                            *f = Some(Instant::now());
                        }
                        stop();
                    });
                    run(victim.as_mut(), source, None)
                }
                // SIGKILL the worker after `after` (child only).
                "kill" => {
                    let kill = victim.killer().ok_or("kill needs a child worker")?;
                    let fired2 = Arc::clone(&fired);
                    thread::spawn(move || {
                        thread::sleep(after);
                        if let Ok(mut f) = fired2.lock() {
                            *f = Some(Instant::now());
                        }
                        kill();
                    });
                    run(victim.as_mut(), source, None)
                }
                // Cooperative: 10k-fuel slices (the facade's async driver's
                // slice); the host checks a cancel flag between slices.
                "slice" => {
                    let flag = Arc::new(AtomicBool::new(false));
                    let (flag2, fired2) = (Arc::clone(&flag), Arc::clone(&fired));
                    thread::spawn(move || {
                        thread::sleep(after);
                        if let Ok(mut f) = fired2.lock() {
                            *f = Some(Instant::now());
                        }
                        flag2.store(true, Ordering::SeqCst);
                    });
                    let mut r = run(victim.as_mut(), source, Some(10_000));
                    let mut slices = 0u64;
                    while matches!(&r, Ok((Response::Parked { reason }, _)) if reason == "fuel")
                        && !flag.load(Ordering::SeqCst)
                    {
                        slices += 1;
                        r = victim.call(
                            &Request::Resume {
                                resume: ResumeMsg::Continue,
                                fuel: Some(10_000),
                            },
                            &[],
                        );
                    }
                    record["slices"] = json!(slices);
                    r
                }
                // No guard in the harness: an OS limit (prlimit) must act.
                "none" => run(victim.as_mut(), source, None),
                // Wasmtime's own limits: the mode's epoch budget or fuel
                // traps the call.
                "wasm" => run(victim.as_mut(), source, None),
                other => return Err(format!("unknown guard {other}")),
            };
            let returned = Instant::now();
            record["victim"] = describe(&result);
            record["control_returned_ms"] = json!(ms(returned - start));
            if let Ok(f) = fired.lock() {
                if let Some(f) = *f {
                    record["latency_ms"] = json!(ms(returned.saturating_duration_since(f)));
                }
            }
            if matches!(&result, Ok((Response::Parked { .. }, _))) {
                record["cancel"] = describe(&victim.call(&Request::Cancel, &[]));
            }
        }
        other => return Err(format!("unknown case {other}")),
    }
    record["elapsed_ms"] = json!(ms(start.elapsed()));
    // The victim's session afterwards.
    let after = victim.call(
        &Request::Run {
            source: "print(2)".into(),
            fuel: Some(u64::MAX),
        },
        &[],
    );
    record["victim_after"] = describe(&after);
    // The other session in the same host.
    record["bystander"] = json!(match finish(bystander.as_mut(), json!("{\"i\": 99}")) {
        Ok(out) if out == CODEMODE_OUT => "completed".to_owned(),
        Ok(out) => format!("wrong output: {out}"),
        Err(e) => format!("lost: {e}"),
    });
    // A new session.
    record["fresh"] = json!(match mode.backend().and_then(|mut b| {
        drive::new_session(b.as_mut(), MEMORY)?;
        drive::run(b.as_mut(), "print(3)", Drive::default())
    }) {
        Ok(out) if drive::output(&out) == "3\n" => "ok".to_owned(),
        Ok(out) => format!("{:?}", out.response),
        Err(e) => format!("failed: {e}"),
    });
    record["host"] = json!("survived");
    Ok(record)
}
