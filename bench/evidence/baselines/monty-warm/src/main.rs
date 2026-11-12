//! Public-core-API probes. Numeric mode times compiled reuse with a fresh heap;
//! the original file modes remain correctness-only boundary checks.
use monty::{MontyRepl, MontyRun};
use monty_types::{CompileOptions, MontyObject, PrintWriter, ResourceLimits, ResourceTracker};
use std::{
    env, fs,
    time::{Duration, Instant},
};

fn tracker() -> ResourceTracker {
    ResourceTracker::new(ResourceLimits {
        max_feed_duration: Some(Duration::from_secs(10)),
        ..ResourceLimits::default()
    })
}

fn numeric(n: u64, repetitions: usize) -> Result<(), Box<dyn std::error::Error>> {
    if n > 10_000_000 || !(1..=1000).contains(&repetitions) {
        return Err("numeric sizes out of bounds".into());
    }
    let expected = MontyObject::int((n * n.saturating_sub(1) / 2) as i64);
    let source = "def numeric_loop(n):\n    total = 0\n    for i in range(n):\n        total += i\n    return total\nnumeric_loop(n)";
    let start = Instant::now();
    let mut code = MontyRun::new(
        source.into(),
        "numeric.py",
        vec!["n".into()],
        CompileOptions::default(),
    )?;
    let compile_ns = start.elapsed().as_nanos();
    let mut times = Vec::with_capacity(repetitions + 1);
    for _ in 0..=repetitions {
        let start = Instant::now();
        let mut output = String::new();
        let result = code.run(
            vec![MontyObject::int(n as i64)],
            tracker(),
            PrintWriter::collect_string(&mut output),
        )?;
        assert_eq!(result, expected, "numeric checksum");
        assert!(output.is_empty());
        times.push(start.elapsed().as_nanos());
    }
    println!(
        "{{\"mode\":\"compiled_fresh_heap\",\"n\":{n},\"checked_result\":{},\"compile_ns\":{compile_ns},\"first_ns\":{},\"warm_ns\":{:?}}}",
        n * n.saturating_sub(1) / 2,
        times[0],
        &times[1..]
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() == 4 && args[1] == "numeric" {
        return numeric(args[2].parse()?, args[3].parse()?);
    }
    if args.len() != 4 || !matches!(args[1].as_str(), "compiled" | "session") {
        return Err(
            "usage: mm-evidence-monty-warm compiled|session SOURCE.py EXPECTED_STDOUT".into(),
        );
    }
    let source = fs::read_to_string(&args[2])?;
    if args[1] == "compiled" {
        eprintln!("boundary compile: MontyRun::new (source parse + prepare; no execution heap)");
        let mut code = MontyRun::new(source, &args[2], vec![], CompileOptions::default())?;
        for index in 0..2 {
            let mut output = String::new();
            eprintln!(
                "boundary evaluate {index}: MontyRun::run (fresh heap + execute + collect + convert)"
            );
            code.run(vec![], tracker(), PrintWriter::collect_string(&mut output))?;
            assert_eq!(output.trim_end(), args[3], "stdout checksum");
            println!("{index}: {}", output.trim_end());
        }
    } else {
        eprintln!("boundary session: MontyRepl::new (persistent heap; no guest compilation)");
        let mut session = MontyRepl::new(&args[2], tracker(), CompileOptions::default());
        for index in 0..2 {
            let mut output = String::new();
            eprintln!(
                "boundary feed {index}: MontyRepl::feed_run (compile + execute fused; retained globals)"
            );
            session.feed_run(&source, vec![], PrintWriter::collect_string(&mut output))?;
            assert_eq!(output.trim_end(), args[3], "stdout checksum");
            println!("{index}: {}", output.trim_end());
        }
    }
    Ok(())
}
