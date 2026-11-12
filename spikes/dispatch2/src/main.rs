//! Spike S2: a redesigned execution core for micro-monty (not shipped).
//! See `README.md` and `docs/spikes/dispatch2.md`.

#![forbid(unsafe_code)]
// The lean build (`--no-default-features`) leaves the extra hot opcodes out.
#![cfg_attr(not(feature = "full_hot"), allow(dead_code, unused_macros))]

mod code;
mod heap;
mod kernels;
mod snapshot;
mod value;
mod vm;

use std::time::Instant;

use heap::exc;
use kernels::{KERNELS, Kernel};
use value::Value;
use vm::{Flow, Park, Vm};

fn kernel(name: &str) -> &'static Kernel {
    KERNELS.iter().find(|k| k.name == name).unwrap_or_else(|| {
        eprintln!("unknown kernel {name}");
        std::process::exit(2)
    })
}

/// Runs to completion (park requests are resumed in place).
fn finish(vm: &mut Vm) -> Flow {
    loop {
        match vm.run() {
            Flow::Park(Park::Requested) => {}
            f => return f,
        }
    }
}

fn run_kernel(k: &Kernel) -> (String, Vm) {
    let mut vm = kernels::build(k);
    match finish(&mut vm) {
        Flow::Done(v) => ((k.show)(&vm, v), vm),
        f => (format!("{f:?}"), vm),
    }
}

/// Parks every `every` ticks for the first `max` parks (then runs free);
/// each park is snapshotted, the VM dropped and a fresh one built from the
/// program and the snapshot. Returns the result, restores and total ticks.
fn run_parked(k: &Kernel, every: u64, max: u64, gc_stress: bool) -> (String, u64, u64) {
    let fresh = |fuel: Option<u64>| {
        let mut vm = kernels::build(k);
        vm.gc_stress = gc_stress;
        vm.fuel = fuel;
        vm
    };
    let mut vm = fresh(Some(every));
    vm.start(0);
    let mut restores = 0;
    loop {
        match vm.run() {
            Flow::Park(Park::Fuel) => {
                let snap = snapshot::save(&vm);
                drop(vm);
                restores += 1;
                vm = fresh(None);
                if let Err(e) = snapshot::restore(&mut vm, &snap) {
                    return (format!("restore failed: {e}"), restores, 0);
                }
                // The host's next slice.
                vm.refuel((restores < max).then_some(every));
            }
            Flow::Done(v) => return ((k.show)(&vm, v), restores, vm.ticks_used),
            f => return (format!("{f:?}"), restores, vm.ticks_used),
        }
    }
}

fn expected_ticks(name: &str) -> u64 {
    match name {
        // 2*F(28) - 1 calls of fib
        "fib" => 635_621,
        "loop" => 3_000_000,
        // 1 make_adder call + 1M add calls + 1M loop tails
        "closure" => 2_000_001,
        "callfn" => 2_000_001,
        "dictint" => 400_000,
        "loopfn" => 1_000_001,
        _ => 0,
    }
}

fn test() -> bool {
    let mut ok = true;
    let mut check = |what: String, good: bool| {
        println!("{} {what}", if good { "ok  " } else { "FAIL" });
        ok &= good;
    };
    for k in KERNELS {
        let (out, vm) = run_kernel(k);
        check(
            format!("{}: {out} (expected {})", k.name, k.expected),
            out == k.expected,
        );
        let t = expected_ticks(k.name);
        check(
            format!("{} ticks {} (expected {t})", k.name, vm.ticks_used),
            vm.ticks_used == t,
        );
        check(format!("{} returned to depth 0", k.name), vm.st.depth == 0);
    }
    for k in KERNELS {
        let t = expected_ticks(k.name);
        for (every, max) in [(1, 3000), (7, 1000), (4999, u64::MAX), (100_003, u64::MAX)] {
            let (out, n, ticks) = run_parked(k, every, max, false);
            check(
                format!(
                    "{} parked every {every} tick(s) (first {} parks): {out}, {n} restores, {ticks} ticks",
                    k.name,
                    max.min(t / every)
                ),
                out == k.expected && ticks == t && n == (t / every).min(max),
            );
        }
        let (out, n, _) = run_parked(k, 50_021, u64::MAX, true);
        check(
            format!(
                "{} parked every 50021 with GC at every safepoint: {out} ({n} restores)",
                k.name
            ),
            out == k.expected,
        );
    }
    // Recursion limit raises RecursionError, caught one frame up the module.
    let t = Kernel {
        name: "recursion",
        expected: "-1",
        build: kernels::t_recursion,
        show: |_, v| v.int_val().to_string(),
    };
    let mut vm = kernels::build(&t);
    let r = finish(&mut vm);
    let good = matches!(r, Flow::Done(v) if v == Value::int(-1));
    check(
        format!(
            "RecursionError at depth {} caught: {r:?}; stack grew to {} words",
            vm.limit,
            vm.stack.len()
        ),
        good,
    );
    // Same, with a lower limit.
    let mut vm = kernels::build(&t);
    vm.limit = 50;
    let r = finish(&mut vm);
    check(
        format!("recursion limit 50: {r:?}"),
        matches!(r, Flow::Done(v) if v == Value::int(-1)),
    );
    // Generic range path (iterator object) and an empty fast range.
    let t = Kernel {
        name: "range",
        expected: "45",
        build: kernels::t_generic_range,
        show: |_, v| v.int_val().to_string(),
    };
    let (out, _) = run_kernel(&t);
    check(
        format!("range(0, 10, 1) through an iterator object, empty range(5, 5): {out}"),
        out == "45",
    );
    // Uncaught slow-path error three frames down.
    let t = Kernel {
        name: "uncaught",
        expected: "",
        build: kernels::t_uncaught,
        show: |_, v| format!("{v:?}"),
    };
    let mut vm = kernels::build(&t);
    let r = finish(&mut vm);
    let kind = if let Flow::Error(e) = r {
        vm.heap.exc_kind(e)
    } else {
        -1
    };
    check(
        format!(
            "i48 overflow raised from depth 5 is uncaught: kind {kind}, depth {}",
            vm.st.depth
        ),
        kind == exc::OVERFLOW && vm.st.depth == 0,
    );
    ok
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("run") => {
            let k = kernel(args.get(2).map_or("fib", String::as_str));
            let reps: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
            for _ in 0..reps {
                let t = Instant::now();
                let (out, _) = run_kernel(k);
                let dt = t.elapsed();
                if reps > 1 {
                    eprintln!("{} {:.3} ms", k.name, dt.as_secs_f64() * 1e3);
                }
                if out != k.expected {
                    eprintln!("{}: wrong result {out}", k.name);
                    std::process::exit(1);
                }
                println!("{}: {out}", k.name);
            }
        }
        Some("test") => {
            if !test() {
                std::process::exit(1);
            }
        }
        #[cfg(feature = "count")]
        Some("stats") => {
            for k in KERNELS {
                let (out, vm) = run_kernel(k);
                println!(
                    "{} dispatched {} ticks {} result {out}",
                    k.name, vm.count, vm.ticks_used
                );
            }
        }
        _ => {
            eprintln!("usage: dispatch2 run <kernel> [reps] | test | stats (--features count)");
            std::process::exit(2);
        }
    }
}
