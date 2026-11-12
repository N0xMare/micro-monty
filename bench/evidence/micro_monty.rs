//! Public-facade evidence probe, compiled externally against an identified rlib.
//! No engine internals, guest output, or filesystem reads enter warm timers.
use micro_monty::{Engine, EngineConfig, HostModule, HostValue, Limits, Progress, Resume, Session};
use std::time::Instant;

type Result<T> = std::result::Result<T, String>;

fn integer(progress: Progress, expected: i64) -> Result<()> {
    if matches!(&progress, Progress::Complete(Some(c)) if c.value == Some(HostValue::Int(expected)))
    {
        Ok(())
    } else {
        Err(format!("expected {expected}, got {progress:?}"))
    }
}

fn engine(memory: u64) -> Engine {
    Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)))
}

fn memory() -> Result<String> {
    let status = std::fs::read_to_string("/proc/self/status").map_err(|e| e.to_string())?;
    let kib = |name| -> Result<u64> {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|v| v.split_whitespace().next())
            .ok_or_else(|| format!("missing {name}"))?
            .parse()
            .map_err(|e| format!("{e}"))
    };
    Ok(format!(
        "\"rss_kib\":{},\"peak_rss_kib\":{}",
        kib("VmRSS:")?,
        kib("VmHWM:")?
    ))
}

fn warm(n: u64, reps: usize) -> Result<()> {
    let expected = i64::try_from(n * n.saturating_sub(1) / 2).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let engine = engine(64 << 20);
    let mut session = Session::new(&engine).map_err(|e| e.traceback)?;
    let setup_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let definition = engine.compile("def numeric_loop(n):\n    total = 0\n    for i in range(n):\n        total += i\n    return total", "numeric.py").map_err(|e| e.to_string())?;
    let call = engine
        .compile(&format!("numeric_loop({n})"), "call.py")
        .map_err(|e| e.to_string())?;
    let compile_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let progress = session.start(&definition);
    if !matches!(progress, Progress::Complete(_)) {
        return Err(format!("definition: {progress:?}"));
    }
    let initialize_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    integer(session.start(&call), expected)?;
    let first_ns = start.elapsed().as_nanos();
    let mut times = Vec::with_capacity(reps);
    for _ in 0..reps {
        let start = Instant::now();
        integer(session.start(&call), expected)?;
        times.push(start.elapsed().as_nanos());
    }
    println!(
        "{{\"mode\":\"warm\",\"n\":{n},\"repetitions\":{reps},\"checked_result\":{expected},\"setup_ns\":{setup_ns},\"compile_ns\":{compile_ns},\"initialize_ns\":{initialize_ns},\"first_ns\":{first_ns},\"warm_ns\":{times:?},{}}}",
        memory()?
    );
    Ok(())
}

fn sessions(rows: u64, count: usize) -> Result<()> {
    let engine = engine(64 << 20);
    let code = engine
        .compile(
            &format!("data = [{{'i': i, 's': 'tag'}} for i in range({rows})]\nlen(data)"),
            "retained.py",
        )
        .map_err(|e| e.to_string())?;
    let baseline = memory()?;
    let mut sessions = Vec::with_capacity(count);
    let start = Instant::now();
    for _ in 0..count {
        let mut session = Session::new(&engine).map_err(|e| e.traceback)?;
        integer(session.start(&code), rows as i64)?;
        sessions.push(session);
    }
    let elapsed_ns = start.elapsed().as_nanos();
    println!(
        "{{\"mode\":\"sessions\",\"rows_per_session\":{rows},\"sessions\":{count},\"setup_total_ns\":{elapsed_ns},\"before\":{{{baseline}}},\"live\":{{{}}}}}",
        memory()?
    );
    // Keep every session alive until after physical-memory observation.
    drop(sessions);
    Ok(())
}

fn resource(limit: u64, bytes: usize) -> Result<()> {
    let engine = engine(limit);
    let mut session = match Session::new(&engine) {
        Ok(s) => s,
        Err(e) if e.type_name == "MemoryError" => {
            println!(
                "{{\"mode\":\"resource\",\"limit\":{limit},\"bytes\":{bytes},\"outcome\":\"session_refused\"}}"
            );
            return Ok(());
        }
        Err(e) => return Err(e.traceback),
    };
    let code = engine
        .compile(&format!("data = 'x' * {bytes}\nlen(data)"), "allocation.py")
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    let outcome = match session.start(&code) {
        p @ Progress::Complete(_) => {
            integer(p, bytes as i64)?;
            "complete"
        }
        Progress::Error(e) if e.type_name == "MemoryError" => "memory_error",
        p => return Err(format!("unexpected allocation result: {p:?}")),
    };
    let elapsed_ns = start.elapsed().as_nanos();
    let recovery = engine
        .compile("6 * 7", "recovery.py")
        .map_err(|e| e.to_string())?;
    integer(session.start(&recovery), 42)?;
    println!(
        "{{\"mode\":\"resource\",\"limit\":{limit},\"bytes\":{bytes},\"outcome\":\"{outcome}\",\"recovery\":42,\"elapsed_ns\":{elapsed_ns},{}}}",
        memory()?
    );
    Ok(())
}

// History cells deliberately retain no functions/code. This tests reclamation,
// not the different product contract of retaining every definition forever.
fn history(rows: u64, cells: usize) -> Result<()> {
    let engine = engine(64 << 20);
    let mut session = Session::new(&engine).map_err(|e| e.traceback)?;
    let setup = engine
        .compile(
            &format!("data = [{{'i': i, 's': 'tag'}} for i in range({rows})]\nlen(data)"),
            "setup.py",
        )
        .map_err(|e| e.to_string())?;
    integer(session.start(&setup), rows as i64)?;
    for i in 0..cells {
        let cell = engine
            .compile(&format!("marker = {i}\nmarker"), "history.py")
            .map_err(|e| e.to_string())?;
        integer(session.start(&cell), i as i64)?;
    }
    let query = engine
        .compile("len(data)", "query.py")
        .map_err(|e| e.to_string())?;
    integer(session.start(&query), rows as i64)?;
    let mut query_ns = Vec::new();
    for _ in 0..7 {
        let t = Instant::now();
        integer(session.start(&query), rows as i64)?;
        query_ns.push(t.elapsed().as_nanos());
    }
    let t = Instant::now();
    let image = session.snapshot().map_err(|e| e.to_string())?;
    let snapshot_ns = t.elapsed().as_nanos();
    let live = memory()?;
    let failure = engine
        .compile("raise ValueError('fixture')", "error.py")
        .map_err(|e| e.to_string())?;
    if !matches!(session.start(&failure), Progress::Error(e) if e.type_name == "ValueError") {
        return Err("expected fixture error".into());
    }
    integer(session.start(&query), rows as i64)?;
    session.reset().map_err(|e| e.traceback)?;
    let reset = engine
        .compile("6 * 7", "reset.py")
        .map_err(|e| e.to_string())?;
    integer(session.start(&reset), 42)?;
    println!(
        "{{\"mode\":\"history\",\"rows\":{rows},\"cells\":{cells},\"query_ns\":{query_ns:?},\"snapshot_ns\":{snapshot_ns},\"snapshot_bytes\":{},\"error_recovery\":true,\"reset_recovery\":true,{live}}}",
        image.len()
    );
    Ok(())
}

// Includes argument/result conversion, public driver dispatch and guest work.
// No network/IPC. A retained immutable payload is echoed synchronously.
fn host(bytes: u64, calls: usize) -> Result<()> {
    let engine = engine(64 << 20);
    let mut session = Session::new(&engine).map_err(|e| e.traceback)?;
    session
        .define(&HostModule::untyped("", &["echo"]))
        .map_err(|e| e.traceback)?;
    let code = engine.compile(
        &format!("data = 'x' * {bytes}\ntotal = 0\nfor i in range({calls}):\n    total += len(echo(data))\ntotal"),
        "host.py",
    ).map_err(|e| e.to_string())?;
    let payload = HostValue::Str("x".repeat(bytes as usize));
    let mut elapsed = Vec::new();
    for _ in 0..4 {
        let t = Instant::now();
        let mut progress = session.start(&code);
        let mut seen = 0;
        while let Progress::HostCall {
            ref name,
            ref args,
            ref kwargs,
            ..
        } = progress
        {
            if name != "echo"
                || args != std::slice::from_ref(&payload)
                || !kwargs.is_empty()
                || seen >= calls
            {
                return Err(format!("wrong host call: {progress:?}"));
            }
            seen += 1;
            progress = session
                .resume(Resume::Value(payload.clone()))
                .map_err(|e| e.to_string())?;
        }
        integer(progress, bytes as i64 * calls as i64)?;
        if seen != calls {
            return Err("host count mismatch".into());
        }
        elapsed.push(t.elapsed().as_nanos());
    }
    println!(
        "{{\"mode\":\"host\",\"payload_bytes\":{bytes},\"calls\":{calls},\"first_ns\":{},\"warm_ns\":{:?},{}}}",
        elapsed[0],
        &elapsed[1..],
        memory()?
    );
    Ok(())
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("use warm N REPS | sessions ROWS COUNT | resource LIMIT BYTES | history ROWS CELLS | host BYTES CALLS".into());
    }
    let n = args[1].parse::<u64>().map_err(|e| e.to_string())?;
    let count = args[2].parse::<usize>().map_err(|e| e.to_string())?;
    if n > 64 << 20 || (count == 0 && args[0] != "history") || count > 10_000_000 {
        return Err("probe input out of bounds".into());
    }
    match args[0].as_str() {
        "warm" if n <= 10_000_000 && count <= 1000 => warm(n, count),
        "sessions" if n <= 100_000 && count <= 100 => sessions(n, count),
        "history" if n <= 100_000 && count <= 1000 => history(n, count),
        "host" if n <= 65536 && count <= 1000 => host(n, count),
        "resource" => resource(n, count),
        _ => Err("unsupported probe mode/size".into()),
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}
