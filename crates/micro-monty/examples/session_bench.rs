//! In-process compilation and persistent-session measurements. Run on an idle
//! machine with `cargo run --release -p micro-monty --example session_bench`.
//! Times include allocation and dropping compiled code, not process startup.
//! Snapshot sizes are serialized bytes, not RSS or peak memory.

use micro_monty::{Engine, EngineConfig, Limits, Progress, Session};
use std::{hint::black_box, process::ExitCode, time::Instant};

fn main() -> ExitCode {
    match measure() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn measure() -> Result<(), String> {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(16 << 20)));
    for (name, source, reps) in [
        ("small", "rows = [{'x': i, 'y': str(i)} for i in range(100)]\n[x['y'] for x in rows if x['x'] % 3 == 0]".to_owned(), 1000),
        ("large", format!("x = 0\n{}", "x = x + 1\n".repeat(5000)), 40),
    ] {
        engine.compile(&source, "bench.py").map_err(|e| e.to_string())?;
        let mut times = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            for _ in 0..reps {
                black_box(engine.compile(black_box(&source), "bench.py").map_err(|e| e.to_string())?);
            }
            times.push(start.elapsed().as_nanos() / reps);
        }
        println!("[M] compile {name}: source_bytes={} samples_ns={times:?}", source.len());
    }
    let mut session = Session::new(&engine).map_err(|e| e.traceback)?;
    let source = format!("#{}\npass", "x".repeat(128 * 1024));
    let code = engine
        .compile(&source, "retention.py")
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    for _ in 0..256 {
        match session.start(&code) {
            Progress::Complete(_) => {}
            Progress::Error(e) => return Err(e.traceback),
            _ => return Err("unexpected suspension".to_owned()),
        }
    }
    let elapsed = start.elapsed().as_micros();
    let snapshot = session.snapshot().map_err(|e| e.to_string())?;
    println!(
        "[M] reused_cells=256 elapsed_us={elapsed} snapshot_bytes={}",
        snapshot.len()
    );
    session = engine.restore(&snapshot).map_err(|e| e.to_string())?;
    session.reset().map_err(|e| e.traceback)?;
    println!(
        "[M] restored_reset_snapshot_bytes={}",
        session.snapshot().map_err(|e| e.to_string())?.len()
    );
    Ok(())
}
