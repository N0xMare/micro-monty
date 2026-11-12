//! Deterministic embedding acceptance workload, through the public facade.
//! Run `cargo run --release -p micro-monty --example embedding_bench` on an
//! idle machine. `MM_GC_STRESS=1` also checks collection at every safepoint.
//! Host services are fixtures, not network I/O. Resume timings include host
//! conversion AND execution; serialized bytes are not RSS or a memory bound.

use micro_monty::{
    Code, Engine, EngineConfig, HostError, HostErrorKind, HostModule, HostValue, Limits,
    ParkReason, Progress, Resume, Session,
};
use std::{collections::BTreeMap, process::ExitCode, time::Instant};

const LOAD: &str = "
names = lookup()
rows_by_id = {}
page_number = 0
while True:
    rows = page(page_number)
    if not rows:
        break
    for row in rows:
        key = row['label'].partition(':')[0].strip().lower()
        if row['active'] and row['amount'] >= 50 and key in names:
            rows_by_id[row['id']] = (names[key], row['amount'] - row['fee'])
    page_number += 1
summary = [len(rows_by_id), sum(v[1] for v in rows_by_id.values()),
           sum(k * v[1] for k, v in rows_by_id.items()),
           sum(k * len(v[0]) for k, v in rows_by_id.items())]
history = []
summary
";
const BATCH: &str = "
import asyncio
async def one(i):
    try:
        return await job(i)
    except ValueError:
        return -i
async def main():
    return await asyncio.gather(*(one(i) for i in range(5)))
asyncio.run(main())
";
const QUERY: &str = "
history.append(len(rows_by_id))
[len(history), sum(history)]
";
const CANCEL: &str = "
abort_marker = 23
cleanup = 0
try:
    while True:
        abort_marker += 1
finally:
    cleanup = 1
";

#[derive(Default)]
struct Measurements {
    // Host-owned instrumentation is deliberately outside the measured calls.
    times: BTreeMap<&'static str, (u64, u128, u128)>,
    images: Vec<usize>,
    host_calls: usize,
    fuel_parks: usize,
    stress: bool,
}

impl Measurements {
    fn record(&mut self, stage: &'static str, start: Instant) {
        let ns = start.elapsed().as_nanos();
        let entry = self.times.entry(stage).or_default();
        entry.0 += 1;
        entry.1 += ns;
        entry.2 = entry.2.max(ns);
    }
}

fn compile(engine: &Engine, source: &str, m: &mut Measurements) -> Result<Code, String> {
    let start = Instant::now();
    let code = engine
        .compile(source, "embedding.py")
        .map_err(|e| e.to_string())?;
    m.record("compile", start);
    Ok(code)
}

fn hibernate(
    engine: &Engine,
    mut session: Session,
    m: &mut Measurements,
) -> Result<Session, String> {
    let start = Instant::now();
    let image = session.snapshot().map_err(|e| e.to_string())?;
    m.record("snapshot", start);
    m.images.push(image.len());
    // Do not keep the old VM alive during restore. The host's fixture state
    // and pending future IDs remain external; snapshots do not persist them.
    drop(session);
    let start = Instant::now();
    let mut restored = engine.restore(&image).map_err(|e| e.to_string())?;
    m.record("restore", start);
    restored.set_gc_stress(m.stress);
    Ok(restored)
}

fn refusal() -> HostError {
    HostError {
        kind: HostErrorKind::ValueError,
        message: "fixture refusal".to_owned(),
        data: None,
        class: None,
    }
}

fn page(number: i64) -> HostValue {
    // Four overlapping pages, then an empty page. Their union is 0..530;
    // overwriting duplicate IDs must not double-count the 18 repeated rows.
    let rows = if number == 4 { 0 } else { 137 };
    HostValue::List(
        (number * 131..number * 131 + rows)
            .map(|i| {
                HostValue::Dict(vec![
                    ("id".into(), HostValue::Int(i)),
                    (
                        "label".into(),
                        HostValue::Str(format!(" TEAM-{} : item-{i}", i % 7)),
                    ),
                    ("active".into(), HostValue::Bool(i % 5 != 1)),
                    ("amount".into(), HostValue::Int(i * 17 % 131)),
                    ("fee".into(), HostValue::Int(i % 9)),
                ])
            })
            .collect(),
    )
}

fn run_cell(
    engine: &Engine,
    mut session: Session,
    code: &Code,
    m: &mut Measurements,
) -> Result<(Session, Progress), String> {
    let start = Instant::now();
    let mut progress = session.start(code);
    m.record("start", start);
    let mut next_page = 0;
    let mut futures = Vec::new();
    // A broken fixture/engine must not make the acceptance driver resupply
    // fuel forever. This is a driver limit, not an engine latency guarantee.
    for _ in 0..10_000 {
        let answer = match progress {
            Progress::Complete(_) | Progress::Error(_) => return Ok((session, progress)),
            Progress::Parked {
                reason: ParkReason::Fuel,
            } => {
                m.fuel_parks += 1;
                session.set_fuel(Some(2000));
                Resume::Continue
            }
            Progress::HostCall {
                call,
                name,
                args,
                kwargs,
                ..
            } if kwargs.is_empty() => {
                m.host_calls += 1;
                // Restore while the third page is outstanding, without
                // restarting the cell or reissuing any of the earlier calls.
                if name == "page" && next_page == 2 {
                    session = hibernate(engine, session, m)?;
                }
                let start = Instant::now();
                let answer = match (name.as_str(), args.as_slice()) {
                    ("lookup", []) => Resume::Value(HostValue::Dict(
                        [(0, "amber"), (2, "cyan"), (5, "violet")]
                            .into_iter()
                            .map(|(i, name)| (format!("team-{i}"), HostValue::Str(name.into())))
                            .collect(),
                    )),
                    ("page", [HostValue::Int(n)]) if *n == next_page && *n <= 4 => {
                        next_page += 1;
                        Resume::Value(page(*n))
                    }
                    ("job", [HostValue::Int(n)])
                        if usize::try_from(*n) == Ok(futures.len()) && *n < 5 =>
                    {
                        futures.push(call);
                        Resume::Future
                    }
                    ("refuse", []) => Resume::Error(refusal()),
                    _ => return Err(format!("unexpected host call: {name} {args:?}")),
                };
                m.record("host-fixture", start);
                answer
            }
            Progress::Waiting { calls } if calls == futures && calls.len() == 5 => {
                session = hibernate(engine, session, m)?;
                // Reply in reverse order, including one failure. Gather must
                // still return input order; Python catches only that failure.
                Resume::Batch(
                    futures
                        .drain(..)
                        .zip([0_i64, 1, 2, 3, 4])
                        .rev()
                        .map(|(call, i)| {
                            let result = if i == 2 {
                                Err(refusal())
                            } else {
                                Ok(HostValue::Int(i * i + 3))
                            };
                            (call, result)
                        })
                        .collect(),
                )
            }
            other => return Err(format!("unexpected progress: {other:?}")),
        };
        let start = Instant::now();
        progress = session.resume(answer).map_err(|e| e.to_string())?;
        m.record("resume", start);
    }
    Err("acceptance driver event limit".into())
}

fn expect_ints(progress: &Progress, expected: &[i64]) -> Result<(), String> {
    let value = HostValue::List(expected.iter().copied().map(HostValue::Int).collect());
    if matches!(progress, Progress::Complete(Some(c)) if c.value.as_ref() == Some(&value)) {
        Ok(())
    } else {
        Err(format!("expected {value:?}, got {progress:?}"))
    }
}

fn measure() -> Result<(), String> {
    let mut m = Measurements {
        stress: std::env::var("MM_GC_STRESS").as_deref() == Ok("1"),
        ..Measurements::default()
    };
    let engine = Engine::new(
        EngineConfig::new().with_limits(Limits::new().with_memory(16 << 20).with_fuel(Some(2000))),
    );
    let start = Instant::now();
    let mut session = Session::new(&engine).map_err(|e| e.traceback)?;
    m.record("new-session", start);
    session.set_gc_stress(m.stress);
    session
        .define(&HostModule::untyped(
            "",
            &["lookup", "page", "job", "refuse"],
        ))
        .map_err(|e| e.to_string())?;
    let code = compile(&engine, LOAD, &mut m)?;
    let (mut session, progress) = run_cell(&engine, session, &code, &mut m)?;
    // Independent range-based oracle (not the paginated fixture): filter
    // IDs 0..530 by i%5!=1, i*17%131>=50, i%7 in {0,2,5}; sum net amounts,
    // ID*net and ID*joined-name-length. Also checked on CPython 3.14.
    expect_ints(&progress, &[115, 9985, 2_616_092, 151_130])?;
    session = hibernate(&engine, session, &mut m)?;
    let query = compile(&engine, QUERY, &mut m)?;
    for count in 1..=32 {
        let start = Instant::now();
        let (next, progress) = run_cell(&engine, session, &query, &mut m)?;
        m.record("query-cell", start);
        expect_ints(&progress, &[count, count * 115])?;
        session = next;
    }
    session = hibernate(&engine, session, &mut m)?;
    let code = compile(&engine, BATCH, &mut m)?;
    let (session, progress) = run_cell(&engine, session, &code, &mut m)?;
    expect_ints(&progress, &[3, 4, -2, 12, 19])?;
    let code = compile(&engine, "recovery_marker = 17\nrefuse()", &mut m)?;
    let (mut session, progress) = run_cell(&engine, session, &code, &mut m)?;
    if !matches!(progress, Progress::Error(ref e) if e.type_name == "ValueError" && e.message == "fixture refusal")
    {
        return Err(format!("expected uncaught fixture error: {progress:?}"));
    }
    let code = compile(&engine, CANCEL, &mut m)?;
    session.set_fuel(Some(100));
    let start = Instant::now();
    let progress = session.start(&code);
    m.record("start", start);
    if progress
        != (Progress::Parked {
            reason: ParkReason::Fuel,
        })
    {
        return Err(format!("expected cancellable fuel park: {progress:?}"));
    }
    let start = Instant::now();
    if !session.cancel() {
        return Err("cancel refused active cell".into());
    }
    m.record("cancel", start);
    session.set_fuel(Some(2000));
    session = hibernate(&engine, session, &mut m)?;
    let code = compile(
        &engine,
        "assert abort_marker > 23\n[recovery_marker, cleanup, len(history), summary[1]]",
        &mut m,
    )?;
    let (mut session, progress) = run_cell(&engine, session, &code, &mut m)?;
    // Exceptions/cancellation are NOT transaction rollback, and cancel does
    // NOT execute guest finally blocks. Both preserve earlier completed cells.
    expect_ints(&progress, &[17, 0, 32, 9985])?;
    let start = Instant::now();
    session.reset().map_err(|e| e.traceback)?;
    m.record("reset", start);
    let code = compile(
        &engine,
        "try:\n    rows_by_id\nexcept NameError:\n    pass\nelse:\n    raise AssertionError('reset kept rows')\n[len(lookup())]",
        &mut m,
    )?;
    let (session, progress) = run_cell(&engine, session, &code, &mut m)?;
    expect_ints(&progress, &[3])?;
    let _session = hibernate(&engine, session, &mut m)?;
    if m.host_calls != 13 || m.fuel_parks == 0 {
        return Err(format!(
            "unexpected host/fuel counts: {} / {}",
            m.host_calls, m.fuel_parks
        ));
    }
    println!(
        "embedding ok gc_stress={} host_calls={} fuel_parks={} snapshot_bytes={:?}",
        m.stress, m.host_calls, m.fuel_parks, m.images
    );
    for (stage, (calls, total, max)) in m.times {
        println!("[M] {stage} calls={calls} total_ns={total} max_ns={max}");
    }
    Ok(())
}

fn main() -> ExitCode {
    match measure() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
