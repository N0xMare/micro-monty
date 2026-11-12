//! External probes of the pinned djs public API, not a replacement CLI.
use std::path::Path;
use std::time::Instant;

use djs_core::compiler::CompileOptions;
use djs_core::heap::Handle;
use djs_core::value::Value;
use djs_core::vm::dump::DumpOptions;
use djs_core::vm::dump_load::LoadOptions;
use djs_core::vm::{Status, Vm, VmOptions};

type Result<T> = std::result::Result<T, String>;

fn debug_error(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}

fn ensure(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn fresh() -> Result<Vm> {
    djs_prelude::new_vm(VmOptions::default()).map_err(debug_error)
}

fn compile(vm: &mut Vm, source: &[u8], filename: &str) -> Result<Value> {
    vm.compile(source, &CompileOptions::script(filename))
        .map_err(debug_error)
}

fn start(vm: &mut Vm, f: Value, args: &[Value]) -> Result<()> {
    vm.call(f, Value::object(vm.global()), args)
        .map_err(debug_error)
}

fn done(vm: &mut Vm) -> Result<Value> {
    match vm.run(u64::MAX) {
        Status::Done(v) => Ok(v),
        Status::Threw(v) => Err(vm.display(v)),
        other => Err(format!("expected Done, got {other:?}")),
    }
}

fn numeric(vm: &mut Vm, f: Handle, n: u64, expected: f64) -> Result<()> {
    let f = vm.heap().get_handle(f);
    start(vm, f, &[Value::number(n as f64)])?;
    let value = done(vm)?;
    ensure(
        value.as_number() == Some(expected),
        "numeric result mismatch",
    )
}

fn elapsed(start: Option<Instant>) -> Option<u128> {
    start.map(|s| s.elapsed().as_nanos())
}

fn warm(n: u64, repetitions: usize, timed: bool) -> Result<()> {
    ensure(
        n <= 10_000_000,
        "n must be at most 10000000 (exact binary64 checksum)",
    )?;
    ensure(
        (1..=100_000).contains(&repetitions),
        "repetitions must be 1..100000",
    )?;
    let clock = || timed.then(Instant::now);
    let t = clock();
    let mut vm = fresh()?;
    let setup_ns = elapsed(t);
    let t = clock();
    let entry = compile(
        &mut vm,
        include_bytes!("numeric-loop.js"),
        "numeric-loop.js",
    )?;
    let compile_ns = elapsed(t);
    let t = clock();
    start(&mut vm, entry, &[])?;
    let f = done(&mut vm)?;
    let f = vm.heap_mut().new_handle(f).map_err(debug_error)?;
    let initialize_ns = elapsed(t);
    let expected = (n as f64) * (n.saturating_sub(1) as f64) / 2.0;
    let t = clock();
    numeric(&mut vm, f, n, expected)?;
    let first_ns = elapsed(t);
    let mut warm_ns = Vec::with_capacity(repetitions);
    for _ in 0..repetitions {
        let t = clock();
        numeric(&mut vm, f, n, expected)?;
        warm_ns.push(elapsed(t));
    }
    vm.heap_mut().drop_handle(f);
    println!(
        "{{\"mode\":\"{}\",\"n\":{n},\"warm_repetitions\":{repetitions},\"checked_result\":{expected}}}",
        if timed { "timed" } else { "check-only" }
    );
    if timed {
        // This is a separate JSON record: checksums are never timing fields.
        println!(
            "{{\"setup_ns\":{},\"compile_ns\":{},\"initialize_ns\":{},\"first_ns\":{},\"warm_ns\":[{}]}}",
            setup_ns.ok_or("missing setup clock")?,
            compile_ns.ok_or("missing compile clock")?,
            initialize_ns.ok_or("missing initialization clock")?,
            first_ns.ok_or("missing first clock")?,
            warm_ns
                .into_iter()
                .map(|v| v.map_or("null".into(), |v| v.to_string()))
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    Ok(())
}

fn capabilities(directory: &Path) -> Result<()> {
    std::fs::create_dir_all(directory).map_err(debug_error)?;
    let dump_options = DumpOptions {
        base: djs_prelude::BASE,
        ..Default::default()
    };

    // A fuel stop: the raw image preserves the instruction pointer and live locals.
    let mut vm = fresh()?;
    let f = compile(
        &mut vm,
        b"let total = 0; for (let i = 0; i < 1000; i++) total += i; total;",
        "fuel.js",
    )?;
    start(&mut vm, f, &[])?;
    ensure(vm.run(17) == Status::OutOfFuel, "expected fuel suspension")?;
    let refused = vm
        .dump(&dump_options, None)
        .err()
        .ok_or("fuel-stop dump unexpectedly accepted")?;
    ensure(
        refused.message().contains("not idle"),
        "wrong fuel-stop refusal",
    )?;
    std::fs::write(directory.join("fuel-dump-refusal.txt"), refused.message())
        .map_err(debug_error)?;
    let image = vm.image().to_vec();
    let fuel_image_len = image.len();
    std::fs::write(directory.join("fuel.img"), &image).map_err(debug_error)?;
    let mut restored = Vm::from_bytes(image, vm.options()).map_err(debug_error)?;
    ensure(
        done(&mut restored)?.as_number() == Some(499500.0),
        "fuel image result mismatch",
    )?;
    println!(
        "fuel_image: verified restore, result=499500 bytes={fuel_image_len}; semantic dump refused (not idle)"
    );

    // A parked synchronous host call: restore, deliver its result, finish.
    let mut vm = fresh()?;
    let host = vm.new_host_function(7, "syncPause").map_err(debug_error)?;
    vm.define_global("syncPause", host).map_err(debug_error)?;
    let f = compile(
        &mut vm,
        b"const before = 9; const answer = syncPause(9); before + answer;",
        "sync-park.js",
    )?;
    start(&mut vm, f, &[])?;
    let call_id = match vm.run(u64::MAX) {
        Status::HostCall { call_id, fn_id: 7 } => call_id,
        other => return Err(format!("expected sync host park, got {other:?}")),
    };
    let refused = vm
        .dump(&dump_options, None)
        .err()
        .ok_or("sync-park dump unexpectedly accepted")?;
    ensure(
        refused.message().contains("not idle"),
        "wrong sync-park refusal",
    )?;
    std::fs::write(directory.join("sync-dump-refusal.txt"), refused.message())
        .map_err(debug_error)?;
    let image = vm.image().to_vec();
    std::fs::write(directory.join("sync-park.img"), &image).map_err(debug_error)?;
    let mut restored = Vm::from_bytes(image, vm.options()).map_err(debug_error)?;
    let call = restored.host_call(call_id).ok_or("restored call missing")?;
    ensure(
        call.fn_id == 7 && call.args.len() == 1 && call.args[0].as_number() == Some(9.0),
        "restored sync call mismatch",
    )?;
    restored
        .resolve_host_call(call_id, Ok(Value::int(33)))
        .map_err(debug_error)?;
    ensure(
        done(&mut restored)?.as_number() == Some(42.0),
        "sync image result mismatch",
    )?;
    println!(
        "sync_park_image: verified restore, preserved host id/argument, result=42; semantic dump refused (not idle)"
    );

    // An async host wait reaches idle, where format-2 continuation dumps are supported.
    let mut vm = fresh()?;
    let host = vm
        .new_async_host_function(8, "later")
        .map_err(debug_error)?;
    vm.define_global("later", host).map_err(debug_error)?;
    let f = compile(&mut vm, include_bytes!("async-park.js"), "async-park.js")?;
    start(&mut vm, f, &[])?;
    ensure(vm.run(u64::MAX) == Status::Idle, "expected async idle")?;
    let calls = vm.pending_host_calls();
    ensure(calls.len() == 1, "expected one async call")?;
    let call_id = calls[0];
    let host_name = |id| if id == 8 { Some("later".into()) } else { None };
    let options = DumpOptions {
        base: djs_prelude::BASE,
        host: &["later"],
        host_fn_name: &host_name,
        ..Default::default()
    };
    let dump = vm.dump(&options, None).map_err(|e| e.message())?;
    ensure(
        dump.starts_with("\"djs-dump 2\";"),
        "unexpected dump format",
    )?;
    std::fs::write(directory.join("async.dump.js"), &dump).map_err(debug_error)?;
    let mut restored = fresh()?;
    let host = restored
        .new_async_host_function(8, "later")
        .map_err(debug_error)?;
    restored.define_global("later", host).map_err(debug_error)?;
    let host_id = |name: &str| if name == "later" { Some(8) } else { None };
    let loaded = restored
        .load_dump(
            &dump,
            &LoadOptions {
                host_fn: &host_id,
                ..Default::default()
            },
        )
        .map_err(|e| e.message())?;
    ensure(
        loaded.calls.len() == 1 && loaded.calls[0].id == call_id && loaded.calls[0].fn_id == 8,
        "loaded async call mismatch",
    )?;
    let call = restored
        .host_call(call_id)
        .ok_or("loaded async call missing")?;
    ensure(
        call.is_async && call.args.len() == 1 && call.args[0].as_number() == Some(33.0),
        "loaded async argument mismatch",
    )?;
    restored
        .resolve_host_call(call_id, Ok(Value::int(33)))
        .map_err(debug_error)?;
    match restored.run(u64::MAX) {
        Status::Done(_) | Status::Idle => {}
        other => return Err(format!("unexpected resumed async status {other:?}")),
    }
    let f = compile(&mut restored, b"probeResult;", "check-result.js")?;
    start(&mut restored, f, &[])?;
    ensure(
        done(&mut restored)?.as_number() == Some(42.0),
        "dump resumed result mismatch",
    )?;
    println!(
        "async_idle_dump: format=2 bytes={} fresh-VM load, preserved host id/argument, result=42",
        dump.len()
    );
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("check" | "warm") if args.len() == 4 => match (args[2].parse(), args[3].parse()) {
            (Ok(n), Ok(reps)) => warm(n, reps, args[1] == "warm"),
            _ => Err("expected integer n and repetition count".into()),
        },
        Some("capabilities") if args.len() == 3 => capabilities(Path::new(&args[2])),
        _ => Err("usage: djs-probe check|warm N WARM_REPETITIONS | capabilities OUTPUT_DIR".into()),
    };
    if let Err(error) = result {
        eprintln!("djs-probe: {error}");
        std::process::exit(1);
    }
}
