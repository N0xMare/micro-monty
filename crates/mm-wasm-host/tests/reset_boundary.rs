//! A reset is a boundary on the mm-wasm guest as on the native facade
//! (`crates/micro-monty/tests/reset_boundary.rs`): every probe of
//! `conformance/reset/probes.txt`, the walk among them, through this host.
//!
//! Needs a guest: `MM_WASM_GUEST`, else the root workspace's release
//! build for wasm32-unknown-unknown
//! (`cargo build --release -p mm-wasm-guest --target wasm32-unknown-unknown`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests"
)]

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use micro_monty::{Limits, Progress, Resume};
use mm_wasm_host::{Runtime, RuntimeConfig, Session, SessionOptions};

#[path = "../../../conformance/reset/probes.rs"]
#[allow(
    dead_code,
    missing_docs,
    reason = "shared with the native facade's test"
)]
mod probes;

fn guest() -> PathBuf {
    std::env::var_os("MM_WASM_GUEST").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/release/mm_wasm_guest.wasm")
        },
        PathBuf::from,
    )
}

struct Guest(Session);

impl probes::Cells for Guest {
    fn start(&mut self, source: &str) -> Progress {
        self.0.start(source, "cell.py").unwrap()
    }

    fn resume(&mut self, answer: Resume) -> Progress {
        self.0.resume(&answer).unwrap()
    }

    fn reset(&mut self) {
        self.0.reset().unwrap();
    }
}

fn read(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/reset")
        .join(name);
    std::fs::read_to_string(path).expect("conformance/reset")
}

#[test]
fn a_cell_after_a_reset_reports_what_it_reports_in_a_new_session() {
    let rt = Runtime::new(&guest(), RuntimeConfig::default()).unwrap();
    let options = SessionOptions::new().with_limits(Limits::new().with_memory(64 << 20));
    let new = |out: &Arc<Mutex<String>>| -> Box<dyn probes::Cells> {
        let mut s = rt.session(&options).unwrap();
        for module in probes::modules() {
            s.define(&module).unwrap();
        }
        let sink = Arc::clone(out);
        s.set_output(Box::new(move |_, t| sink.lock().unwrap().push_str(t)));
        Box::new(Guest(s))
    };
    let probes = probes::probes(&read("probes.txt"), &read("walk.py"));
    match probes::check(&probes, &new) {
        Ok(n) => assert!(n > 60, "{n} probes"),
        Err(failures) => panic!("{failures}"),
    }
}
