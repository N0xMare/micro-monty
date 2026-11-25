//! One isolated child of `fuzz/budget_sweep.py`: runs one own case with the
//! Nth charge against the memory budget refused (0: none), then a canary
//! cell in the same session. Prints `charges<TAB>outcome<TAB>detail`, then
//! the cell's printed output. Outcomes: `complete`, `exception` (detail: the
//! uncaught type), `caught` (the `--catch` control's handler ran), `stuck`
//! (the session cannot run the canary or continue) and `fuel` (the cell ran
//! past the sweep's fuel). An uncaught exception's report goes to stderr.
//! A panic or abort is the process's exit status.

use micro_monty::{Code, Engine, EngineConfig, Limits, ParkReason, Progress, Resume, Session};
use mm_core::budget_fault;
use std::sync::{Arc, Mutex};

const CONTROL: &str = "\
ran_handler = False
try:
    import statistics
except MemoryError:
    recovered = [17, 25]
    assert sum(recovered) == 42
    ran_handler = True
ran_handler
";

const CANARY: &str = "\
def _budget_sweep_canary():
    rows = [[19, 23], [3, 5]]
    return rows[0][0] + rows[0][1]
_budget_sweep_canary()
";

const SLICE: u64 = 1_000_000;
const RESUMES: usize = 2000;

/// The cell's last progress, `Ok(None)` when it ran past the sweep's fuel.
fn drive(session: &mut Session, code: &Code) -> Result<Option<Progress>, String> {
    session.set_fuel(Some(SLICE));
    let mut progress = session.start(code);
    for _ in 0..RESUMES {
        if progress
            != (Progress::Parked {
                reason: ParkReason::Fuel,
            })
        {
            return Ok(Some(progress));
        }
        session.set_fuel(Some(SLICE.max(session.fuel_wanted())));
        progress = session
            .resume(Resume::Continue)
            .map_err(|e| format!("resume: {e}"))?;
    }
    Ok(None)
}

/// Runs the case's cell with the injection armed, then the canary unarmed.
/// Returns the charges the case's cell made, refused or not.
fn outcome(
    session: &mut Session,
    code: &Code,
    canary: &Code,
    control: bool,
) -> (usize, &'static str, String) {
    let progress = drive(session, code);
    let charges = budget_fault::disarm();
    let (kind, detail) = verdict(session, progress, canary, control);
    (charges, kind, detail)
}

fn verdict(
    session: &mut Session,
    progress: Result<Option<Progress>, String>,
    canary: &Code,
    control: bool,
) -> (&'static str, String) {
    let progress = match progress {
        Ok(Some(progress)) => progress,
        Ok(None) => return ("fuel", format!("{RESUMES} slices of {SLICE}")),
        Err(e) => return ("stuck", e),
    };
    let (kind, detail) = match &progress {
        _ if session.is_poisoned() => return ("stuck", format!("poisoned after {progress:?}")),
        Progress::Complete(Some(c)) if control && c.repr == "True" => {
            ("caught", "MemoryError".to_owned())
        }
        Progress::Complete(_) => ("complete", "-".to_owned()),
        Progress::Error(e) => {
            // For a replay by hand; the driver reads only stdout.
            eprint!("{}", e.traceback);
            ("exception", e.type_name.clone())
        }
        other => return ("stuck", format!("unexpected stop: {other:?}")),
    };
    match drive(session, canary) {
        Ok(Some(Progress::Complete(Some(c)))) if c.repr == "42" && !session.is_poisoned() => {
            (kind, detail)
        }
        other => ("stuck", format!("canary after {kind} {detail}: {other:?}")),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("expected CASE.py or --catch")?;
    let at = args.next().ok_or("expected charge index")?.parse()?;
    let control = path == "--catch";
    let source = if control {
        CONTROL.to_owned()
    } else {
        std::fs::read_to_string(&path)?
    };
    let engine =
        Engine::new(EngineConfig::new().with_limits(Limits::new().with_recursion_depth(51)));
    let code = match engine.compile(&source, &path) {
        Ok(code) => code,
        Err(e) => {
            println!("0\tcompile\t{}", e.type_name);
            return Ok(());
        }
    };
    let canary = engine.compile(CANARY, "budget-canary.py")?;
    let mut session = Session::new(&engine)?;
    session.set_wall_clock(Some(Box::new(|| 1_794_000_000.0)));
    let mut entropy = 0u64;
    session.set_entropy(Some(Box::new(move |bytes| {
        for byte in bytes {
            *byte = entropy.wrapping_mul(0x9e37_79b9_7f4a_7c15).to_be_bytes()[0];
            entropy = entropy.wrapping_add(1);
        }
    })));
    let output = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&output);
    session.set_output(Box::new(move |text| sink.lock().unwrap().push_str(text)));

    budget_fault::arm(at);
    let (charges, kind, detail) = outcome(&mut session, &code, &canary, control);
    println!("{charges}\t{kind}\t{}", detail.replace(['\t', '\n'], " "));
    print!("{}", output.lock().unwrap());
    Ok(())
}
