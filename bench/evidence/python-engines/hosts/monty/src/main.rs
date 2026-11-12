//! Monty 1.5.0 (pydantic/monty) under the shared engine harness
//! (bench/evidence/engines/hosts/common), for the `gcpause` command only:
//! the Python-engines study compares GC pauses across all four engines, and
//! Monty is mm's ancestor (its corpus is conformance/monty). A run is one
//! `MontyRun` with Monty's default settings and a 256 MiB `max_memory`, as
//! mm's default limit; `hsync` is an external function the host answers.
//! Monty frees by reference counting and collects cycles by trial deletion
//! every 100,000 GC-tracked allocations (`DEFAULT_GC_INTERVAL`).

use common::{Control, Engine, Host, Lang, RunResult, Stop, ToolDef, Value, json};
use monty::{MontyRun, RunProgress};
use monty_types::{
    CompileOptions, MontyObject, NameLookupResult, PrintWriter, ResourceLimits, ResourceTracker,
};
use std::time::Duration;

struct Monty;

fn run_monty(src: &str) -> RunResult {
    let runner = MontyRun::new(
        src.to_owned(),
        "cell.py",
        vec!["hsync".to_owned()],
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let limits = ResourceLimits::default().max_memory(256 << 20);
    let mut out = String::new();
    let mut p = runner
        .start(
            vec![MontyObject::function("hsync".to_owned(), None)],
            ResourceTracker::new(limits),
            PrintWriter::CollectString(&mut out, None),
        )
        .map_err(|e| e.to_string())?;
    loop {
        p = match p {
            RunProgress::FunctionCall(call) if call.function_name == "hsync" => {
                common::mark();
                // The scripts that call it ignore the result.
                call.resume(
                    MontyObject::none(),
                    PrintWriter::CollectString(&mut out, None),
                )
                .map_err(|e| e.to_string())?
            }
            RunProgress::NameLookup(l) => l
                .resume(
                    NameLookupResult::Undefined,
                    PrintWriter::CollectString(&mut out, None),
                )
                .map_err(|e| e.to_string())?,
            RunProgress::Complete(_) => return Ok(out),
            other => return Err(format!("unsupported progress: {other:?}")),
        };
    }
}

impl Engine for Monty {
    type S = ();
    const NAME: &'static str = "monty";
    const LANG: Lang = Lang::Py;

    fn new_runtime() -> Self {
        Monty
    }
    fn new_session(&mut self) {}
    fn setup(&mut self, _s: &mut (), _tools: &[ToolDef]) {}
    fn run(&mut self, _s: &mut (), src: &str, _host: &mut dyn Host) -> RunResult {
        run_monty(src)
    }
    fn run_plain(&mut self, src: &str) -> RunResult {
        run_monty(src)
    }
    fn control(&mut self, _s: &mut (), _src: &str, _stop: &Stop, _delay: Duration) -> Control {
        common::fail("monty host: gcpause only")
    }
    fn run_polled(&mut self, _src: &str) -> RunResult {
        Err("monty host: gcpause only".into())
    }
    fn run_until(
        &mut self,
        _s: &mut (),
        _src: &str,
        _host: &mut dyn Host,
        _stop_after: usize,
    ) -> bool {
        false
    }
    fn finish(&mut self, _s: &mut (), _host: &mut dyn Host) -> RunResult {
        Err("monty host: gcpause only".into())
    }
    fn describe() -> Value {
        json!({"engine": "monty", "version": "1.5.0", "max_memory": 256 << 20})
    }
}

fn main() {
    common::main::<Monty>();
}
