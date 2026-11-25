//! Run the public native inventory in small fuel slices. See fuzz/README.md.

use micro_monty::{Engine, EngineConfig, ParkReason, Progress, Resume, Session};
use std::collections::BTreeSet;
use std::time::Instant;

// docs/spec.md §5.1, clause 3; the budget is below the VM's tick period.
const OVERDRAFT: u64 = (1 << 21) + (1 << 17);
const SLICE: u64 = 64;
const FUEL_CEILING: u64 = 1 << 31;
const TABLE: &str = include_str!("../../fuel_sweep.txt");

fn inventory() -> BTreeSet<String> {
    (0..)
        .map_while(mm_engine::native)
        .filter_map(|native| match (native.module(), native.owner) {
            (None, Some(owner)) => Some(format!("{}.{}", owner.name(), native.name())),
            (None, None) => Some(native.name().to_owned()),
            (Some("_prelude"), None) if native.name() == "__import__" => {
                Some("__import__".to_owned())
            }
            (Some(module), None) if !module.starts_with('_') && !module.is_empty() => {
                Some(format!("{module}.{}", native.name()))
            }
            _ => None, // Unbound aliases and private interpreter helpers.
        })
        .collect()
}

#[derive(Default)]
struct Measured {
    slices: u64,
    requested: u64,
    worst: u64,
    over: u64,
    exempt: u64,
    terminal: String,
}

fn run(engine: &Engine, setup: &str, src: &str) -> Result<Measured, String> {
    let mut s = Session::new(engine).map_err(|e| e.to_string())?;
    if !setup.is_empty() {
        let code = engine
            .compile(setup, "setup.py")
            .map_err(|e| e.to_string())?;
        let p = s.start(&code);
        if !matches!(p, Progress::Complete(_)) {
            return Err(format!("setup: {p:?}"));
        }
    }
    let code = engine.compile(src, "probe.py").map_err(|e| e.to_string())?;
    let mut measured = Measured::default();
    // The facade saturates the total at zero. Leave enough headroom to
    // measure the last slice exactly even if it crosses the sweep ceiling.
    let total = u64::MAX / 2;
    let mut left = total;
    let mut given = SLICE;
    let mut wanted = 0;
    s.set_fuel_total(Some(left));
    s.set_fuel(Some(given));
    let mut p = s.start(&code);
    loop {
        let now = s.fuel_total().expect("set a total before starting");
        let overdraft = (left - now).saturating_sub(given);
        measured.slices += 1;
        measured.worst = measured.worst.max(overdraft);
        measured.over += u64::from(overdraft > OVERDRAFT);
        // The spec exempts the slice after a park that requested fuel,
        // provided the host granted at least the request.
        measured.exempt += u64::from(overdraft > OVERDRAFT && wanted != 0);
        left = now;
        if !matches!(
            p,
            Progress::Parked {
                reason: ParkReason::Fuel
            }
        ) {
            break;
        }
        wanted = s.fuel_wanted();
        measured.requested += u64::from(wanted != 0);
        given = SLICE.max(wanted);
        if total - left >= FUEL_CEILING || given > FUEL_CEILING {
            return Err(format!(
                "exhausted sweep fuel ceiling ({FUEL_CEILING} units)"
            ));
        }
        s.set_fuel(Some(given));
        p = s.resume(Resume::Continue).map_err(|e| e.to_string())?;
    }
    measured.terminal = match p {
        Progress::Complete(_) if !s.is_poisoned() => "ok".to_owned(),
        Progress::Error(e) if !s.is_poisoned() => e.type_name,
        other => return Err(format!("unexpected stop: {other:?}")),
    };
    Ok(measured)
}

fn sizes(spec: &str) -> Vec<(usize, &str)> {
    let predefined = match spec {
        "-" => "1",
        "items" => "3,131071,131072,131073,1048576",
        "bytes" => "3,524287,524288,524289,8388607,8388608,8388609,67108863",
        _ => spec,
    };
    predefined
        .split(',')
        .map(|point| {
            let (size, expected) = point.split_once(':').unwrap_or((point, "ok"));
            (size.parse().expect("integer fixture size"), expected)
        })
        .collect()
}

fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default();
    let native = inventory();
    if filter == "--inventory" {
        for name in native {
            println!("{name}");
        }
        return;
    }
    let engine = Engine::new(EngineConfig::new());
    let start = Instant::now();
    let mut covered = BTreeSet::new();
    let (mut runs, mut failures) = (0, 0);
    let mut total = Measured::default();
    for line in TABLE
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let parts: Vec<_> = line.splitn(4, '|').map(str::trim).collect();
        let [names, spec, setup, body] = parts.as_slice() else {
            panic!("bad fixture: {line}")
        };
        for key in names.split_whitespace().filter(|key| key.contains(&filter)) {
            covered.insert(key.to_owned());
            let name = key.rsplit('.').next().expect("fixture name");
            for (n, expected) in sizes(spec) {
                let expand = |s: &str| {
                    s.replace("{n}", &n.to_string())
                        .replace("{op}", name)
                        .replace("{key}", key)
                        .replace("\\n", "\n")
                };
                let setup = expand(setup);
                let body = expand(body);
                // Assign results so facade completion rendering does not
                // measure another native on the operation's output.
                let src = body
                    .strip_prefix(':')
                    .map_or_else(|| format!("r = {body}"), str::to_owned);
                runs += 1;
                match run(&engine, &setup, &src) {
                    Ok(m) => {
                        let good = m.over == m.exempt && m.terminal == expected;
                        failures += usize::from(!good);
                        total.slices += m.slices;
                        total.requested += m.requested;
                        total.worst = total.worst.max(m.worst);
                        total.over += m.over;
                        total.exempt += m.exempt;
                        println!(
                            "{} {key} n={n} overdraft={} slices={} wanted={} over={} exempt={} outcome={} expected={expected}",
                            if good { "OK" } else { "FAIL" },
                            m.worst,
                            m.slices,
                            m.requested,
                            m.over,
                            m.exempt,
                            m.terminal
                        );
                        if !good {
                            eprintln!("{key} n={n} setup={setup:?} probe={src:?}");
                        }
                    }
                    Err(error) => {
                        failures += 1;
                        eprintln!("FAIL {key} n={n}: {error}; setup={setup:?} probe={src:?}");
                    }
                }
            }
        }
    }
    let missing: Vec<_> = native
        .iter()
        .filter(|name| name.contains(&filter) && !covered.contains(*name))
        .collect();
    if !missing.is_empty() {
        eprintln!("missing native fixtures: {missing:?}");
    }
    eprintln!(
        "{} fixture names ({} of {} inventory); {runs} runs, {} slices, {} preflight parks; max overdraft {} (bound {OVERDRAFT}), {} over-bound slices ({} exempt), {failures} failed runs; {:.2}s",
        covered.len(),
        covered.intersection(&native).count(),
        native.len(),
        total.slices,
        total.requested,
        total.worst,
        total.over,
        total.exempt,
        start.elapsed().as_secs_f64()
    );
    if failures != 0 || runs == 0 || !missing.is_empty() {
        std::process::exit(1);
    }
}
