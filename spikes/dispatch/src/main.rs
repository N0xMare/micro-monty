//! Spike S1: interpreter-dispatch benchmark for micro-monty.
//!
//! Three bytecode designs (stack, accumulator, three-address register) × two
//! dispatch mechanisms (`match` loop, function-pointer table) over one shared
//! substrate (values, arena, frames, ICs, safepoints, slow paths). See
//! README.md for how every number is produced.
//!
//! Usage:
//!   dispatch bench [--reps N] [--kernel NAME]... [--variant DESIGN/MECH]...
//!   dispatch stats

#![cfg_attr(not(feature = "unchecked"), forbid(unsafe_code))]
#![cfg_attr(feature = "unchecked", deny(unsafe_code))]

#[macro_use]
mod vm;
mod acc;
mod access;
mod heap;
mod kernels;
mod reg3;
mod rt;
mod stack;
mod value;

use std::time::Instant;

use heap::{Fail, Heap};
use kernels::Kernel;
use value::Value;
use vm::{Program, Vm};

type Run = for<'a> fn(Vm<'a>) -> Result<Value, Fail>;

struct Design {
    name: &'static str,
    build: fn(Kernel) -> Program,
    run_match: Run,
    run_fntable: Run,
}

const DESIGNS: [Design; 3] = [
    Design {
        name: "stack",
        build: stack::build,
        run_match: stack::run_match,
        run_fntable: stack::run_fntable,
    },
    Design {
        name: "accumulator",
        build: acc::build,
        run_match: acc::run_match,
        run_fntable: acc::run_fntable,
    },
    Design {
        name: "register3",
        build: reg3::build,
        run_match: reg3::run_match,
        run_fntable: reg3::run_fntable,
    },
];

const MECHS: [&str; 2] = ["match", "fntable"];

/// Frame-stack size in words.
const STACK_WORDS: usize = 1 << 16;

struct Variant {
    label: String,
    design: usize,
    run: Run,
}

fn variants() -> Vec<Variant> {
    let mut v = Vec::new();
    for (d, design) in DESIGNS.iter().enumerate() {
        for mech in MECHS {
            let run = if mech == "match" {
                design.run_match
            } else {
                design.run_fntable
            };
            v.push(Variant {
                label: format!("{}/{}", design.name, mech),
                design: d,
                run,
            });
        }
    }
    v
}

fn features() -> String {
    let mut f = Vec::new();
    if cfg!(feature = "count") {
        f.push("count");
    }
    if cfg!(feature = "unchecked") {
        f.push("unchecked");
    }
    if f.is_empty() {
        "default".to_string()
    } else {
        f.join(",")
    }
}

struct Outcome {
    ms: f64,
    result: Value,
    ticks: u64,
    refills: u64,
    count: u64,
}

/// One run: reset the heap, untimed setup, timed run of the entry code.
fn run_once(prog: &Program, k: Kernel, run: Run, heap: &mut Heap, stack: &mut [Value]) -> Outcome {
    heap.reset(&prog.ic_names);
    let args = kernels::setup(k, prog, heap).unwrap_or_else(|e| fatal(&format!("setup: {e:?}")));
    let vm = Vm::new(prog, heap, stack, &args).unwrap_or_else(|e| fatal(&format!("entry: {e:?}")));
    let t0 = Instant::now();
    let r = run(vm);
    let ms = t0.elapsed().as_secs_f64() * 1e3;
    let result = r.unwrap_or_else(|e| fatal(&format!("{} raised {e:?}", k.name())));
    Outcome {
        ms,
        result,
        ticks: heap.ticks(),
        refills: heap.refills,
        count: heap.count,
    }
}

fn fatal(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}

fn check(label: &str, k: Kernel, o: &Outcome) -> bool {
    let ok = o.result.is_int() && o.result.int_val() == k.expected();
    let got = if o.result.is_int() {
        o.result.int_val().to_string()
    } else {
        format!("{:?}", o.result)
    };
    println!(
        "verify {label:<21} {:<8} result={got} expected={} ticks={} refills={} {}",
        k.name(),
        k.expected(),
        o.ticks,
        o.refills,
        if ok { "OK" } else { "MISMATCH" }
    );
    ok
}

fn median(xs: &[f64]) -> f64 {
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn min(xs: &[f64]) -> f64 {
    xs.iter().copied().fold(f64::INFINITY, f64::min)
}

fn bench(args: &[String]) {
    let mut reps = 15;
    let mut kernel_filter: Vec<String> = Vec::new();
    let mut variant_filter: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--reps" => {
                reps = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| fatal("--reps N"))
            }
            "--kernel" => {
                kernel_filter.push(it.next().cloned().unwrap_or_else(|| fatal("--kernel NAME")))
            }
            "--variant" => {
                variant_filter.push(it.next().cloned().unwrap_or_else(|| fatal("--variant D/M")))
            }
            _ => fatal(&format!("unknown argument {a}")),
        }
    }
    let kernels: Vec<Kernel> = Kernel::ALL
        .into_iter()
        .filter(|k| kernel_filter.is_empty() || kernel_filter.iter().any(|f| f == k.name()))
        .collect();
    let vars: Vec<Variant> = variants()
        .into_iter()
        .filter(|v| variant_filter.is_empty() || variant_filter.contains(&v.label))
        .collect();
    let progs: Vec<Vec<Program>> = DESIGNS
        .iter()
        .map(|d| Kernel::ALL.iter().map(|&k| (d.build)(k)).collect())
        .collect();
    let prog = |v: &Variant, k: Kernel| &progs[v.design][k as usize];

    let mut heap = Heap::new();
    let mut stack = vec![Value::UNDEF; STACK_WORDS];
    println!(
        "# dispatch bench: features={} reps={reps} (+1 untimed warm-up/verification round), \
         rounds interleave kernels and variants",
        features()
    );

    // Warm-up and verification round.
    let mut all_ok = true;
    for &k in &kernels {
        for v in &vars {
            let o = run_once(prog(v, k), k, v.run, &mut heap, &mut stack);
            all_ok &= check(&v.label, k, &o);
        }
    }

    // Timed rounds: every round runs every (kernel, variant) once.
    let mut samples = vec![vec![Vec::with_capacity(reps); vars.len()]; kernels.len()];
    for _ in 0..reps {
        for (ki, &k) in kernels.iter().enumerate() {
            for (vi, v) in vars.iter().enumerate() {
                let o = run_once(prog(v, k), k, v.run, &mut heap, &mut stack);
                if o.result.int_val() != k.expected() || !o.result.is_int() {
                    all_ok = false;
                }
                samples[ki][vi].push(o.ms);
            }
        }
    }
    println!(
        "# all {} timed results match expected: {all_ok}",
        reps * kernels.len() * vars.len()
    );
    println!(
        "# arena segments in use at the end: {}",
        heap.arena.segments()
    );

    println!("\n# summary (ms)");
    println!("variant,kernel,reps,min_ms,median_ms,max_ms");
    for (ki, k) in kernels.iter().enumerate() {
        for (vi, v) in vars.iter().enumerate() {
            let s = &samples[ki][vi];
            let max = s.iter().copied().fold(0.0, f64::max);
            println!(
                "{},{},{reps},{:.2},{:.2},{max:.2}",
                v.label,
                k.name(),
                min(s),
                median(s)
            );
        }
    }

    println!("\n# median ms (min ms)");
    print!("{:<8}", "kernel");
    for v in &vars {
        print!(" | {:>21}", v.label);
    }
    println!();
    for (ki, k) in kernels.iter().enumerate() {
        print!("{:<8}", k.name());
        for s in &samples[ki] {
            print!(" | {:>21}", format!("{:.1} ({:.1})", median(s), min(s)));
        }
        println!();
    }

    println!("\n# raw samples");
    println!("variant,kernel,round,ms");
    for (ki, k) in kernels.iter().enumerate() {
        for (vi, v) in vars.iter().enumerate() {
            for (r, ms) in samples[ki][vi].iter().enumerate() {
                println!("{},{},{r},{ms:.3}", v.label, k.name());
            }
        }
    }
    if !all_ok {
        fatal("result mismatch");
    }
}

fn stats() {
    println!("# dispatch stats: features={}", features());
    println!("\n# static bytecode per kernel (all code objects of the kernel)");
    println!("design,kernel,bytecode_bytes,static_instructions");
    for d in &DESIGNS {
        for k in Kernel::ALL {
            let p = (d.build)(k);
            println!(
                "{},{},{},{}",
                d.name,
                k.name(),
                p.bytecode_bytes(),
                p.static_instructions()
            );
        }
    }
    if !cfg!(feature = "count") {
        println!("\n# dispatched-instruction counts need `--features count`");
        return;
    }
    println!("\n# instructions dispatched per kernel run");
    println!("variant,kernel,dispatched,ticks,result_ok");
    let mut heap = Heap::new();
    let mut stack = vec![Value::UNDEF; STACK_WORDS];
    for v in variants() {
        for k in Kernel::ALL {
            let p = (DESIGNS[v.design].build)(k);
            let o = run_once(&p, k, v.run, &mut heap, &mut stack);
            let ok = o.result.is_int() && o.result.int_val() == k.expected();
            println!("{},{},{},{},{ok}", v.label, k.name(), o.count, o.ticks);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bench") => bench(&args[1..]),
        Some("stats") => stats(),
        _ => {
            eprintln!(
                "usage: dispatch bench [--reps N] [--kernel NAME]... [--variant DESIGN/MECH]...\n       dispatch stats"
            );
            std::process::exit(2);
        }
    }
}
