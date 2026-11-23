//! A reset is a boundary (docs/spec.md §4): nothing a cell does reaches
//! the cells after `Session::reset`. Every probe of
//! `conformance/reset/probes.txt` runs a cell, resets, and runs a second
//! cell, which must report what it reports in a new session; one of them
//! is the walk (`conformance/reset/walk.py`), which writes to everything a
//! cell can reach and looks for the writes after the reset.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{Engine, EngineConfig, Limits, Progress, Resume, Session};
use std::sync::{Arc, Mutex};

#[path = "../../../conformance/reset/probes.rs"]
#[allow(
    dead_code,
    missing_docs,
    reason = "shared with the mm-wasm host's test"
)]
mod probes;

struct Native {
    engine: Engine,
    session: Session,
}

impl probes::Cells for Native {
    fn start(&mut self, source: &str) -> Progress {
        match self.engine.compile(source, "cell.py") {
            Ok(code) => self.session.start(&code),
            Err(e) => panic!("a probe does not compile: {e:?}"),
        }
    }

    fn resume(&mut self, answer: Resume) -> Progress {
        self.session.resume(answer).expect("resumes")
    }

    fn reset(&mut self) {
        self.session.reset().expect("resets");
    }
}

fn session(out: &Arc<Mutex<String>>, restored: bool) -> Box<dyn probes::Cells> {
    let limits = Limits::new().with_memory(64 << 20);
    let engine = Engine::new(EngineConfig::new().with_limits(limits));
    let mut session = Session::new(&engine).expect("session");
    for module in probes::modules() {
        session.define(&module).expect("defined");
    }
    if restored {
        let image = session.snapshot().expect("snapshot");
        session = engine.restore(&image).expect("restores");
    }
    let sink = Arc::clone(out);
    session.set_output(Box::new(move |t: &str| {
        sink.lock().expect("output lock").push_str(t);
    }));
    Box::new(Native { engine, session })
}

fn new(out: &Arc<Mutex<String>>) -> Box<dyn probes::Cells> {
    session(out, false)
}

fn read(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/reset")
        .join(name);
    std::fs::read_to_string(path).expect("conformance/reset")
}

#[test]
fn a_cell_after_a_reset_reports_what_it_reports_in_a_new_session() {
    let probes = probes::probes(&read("probes.txt"), &read("walk.py"), &read("sweep.py"));
    match probes::check(&probes, &new) {
        Ok(n) => assert!(n > 60, "{n} probes"),
        Err(failures) => panic!("{failures}"),
    }
}

/// The same on sessions restored from an image taken after the host's
/// definitions: an image holds what marks them as shared.
#[test]
fn the_same_on_a_restored_session() {
    let probes = probes::probes(&read("probes.txt"), &read("walk.py"), &read("sweep.py"));
    if let Err(failures) = probes::check(&probes, &|out| session(out, true)) {
        panic!("{failures}");
    }
}

/// The walk is not vacuous: without the reset its check finds its marks,
/// on the builtins module's constants and on a bundled module's classes.
#[test]
fn the_walk_reaches_and_marks_what_a_cell_can_write() {
    let found = probes::walk_control(&read("walk.py"), &new);
    let marks: usize = found
        .lines()
        .find_map(|l| l.strip_prefix("marks ")?.split(' ').next()?.parse().ok())
        .expect("the check reports its marks");
    let reached: usize = found
        .lines()
        .find_map(|l| l.strip_prefix("reached ")?.split(' ').next()?.parse().ok())
        .expect("the check reports what it reached");
    assert!(reached > 10_000 && marks > 3_000, "{found}");
    assert!(found.contains("limit False"), "{found}");
    for path in ["'CONF'", "'CONFD'", "'CONFR'", "'_markupbase.ParserBase'"] {
        assert!(found.contains(path), "{path}: {found}");
    }
}

/// A shared class lists no subclasses, so nothing may write one that has
/// a subclass (debug builds fail on such a write, `Vm::class_changed`). A
/// second `define` in a session that holds subclasses of the first one's
/// classes makes new classes and leaves the old ones as they were.
#[test]
fn a_second_define_leaves_the_subclasses_of_the_old_classes_working() {
    let out = Arc::new(Mutex::new(String::new()));
    let limits = Limits::new().with_memory(64 << 20);
    let engine = Engine::new(EngineConfig::new().with_limits(limits));
    let mut session = Session::new(&engine).expect("session");
    for module in probes::modules() {
        session.define(&module).expect("defined");
    }
    let mut s = Native { engine, session };
    let (made, _) = probes::cell(
        &mut s,
        &out,
        "class Sub(Entry):\n    def go(self, n=5):\n        return ('sub', n)\n\
         class SubE(ToolError): pass\n\
         class SubR(type(CONFR)): pass\n\
         old = (Entry, ToolError, type(CONFR))\n\
         s = Sub('p')\n\
         [s.go(), s.path]",
    );
    assert_eq!(made, "[('sub', 5), 'p']");
    for module in probes::modules() {
        s.session.define(&module).expect("defined again");
    }
    let (after, _) = probes::cell(
        &mut s,
        &out,
        "[s.go(), Sub('q').path, Sub.__mro__[1] is old[0], Entry is old[0], ToolError is old[1], \
         isinstance(obj(), Entry), isinstance(obj(), old[0]), SubE('m').args, SubR(1).a]",
    );
    assert_eq!(
        after,
        "[('sub', 5), 'q', True, False, False, True, False, ('m',), 1]"
    );
}
