//! The `mm-cell-worker` loop on stdio with a pinned wall clock and
//! entropy, for `conformance/codemode/real/gate.py`, which replays recorded
//! plak sessions through it. It is the binary without seccomp, its
//! resource report or the crash cuts; the library, the codec and the
//! session settings are the shipped ones.
//!
//! `MM_REPLAY_CLOCK` is the wall clock in seconds since the Unix epoch
//! (default 0); entropy is a counter.

use std::{io, process::ExitCode, thread};

use mm_cell_worker::{NativeEngine, Options};

fn main() -> ExitCode {
    let clock: f64 = std::env::var("MM_REPLAY_CLOCK")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0.0);
    let engine = NativeEngine::with_session_setup(move |session| {
        session.set_wall_clock(Some(Box::new(move || clock)));
        let mut counter = 0u8;
        session.set_entropy(Some(Box::new(move |buf: &mut [u8]| {
            for byte in buf {
                counter = counter.wrapping_add(37);
                *byte = counter;
            }
        })));
    });
    let serve = move || {
        mm_cell_worker::serve(
            Box::new(io::stdin()),
            Box::new(io::stdout()),
            &engine,
            Options::default(),
        )
    };
    // The binary's stack (src/main.rs).
    match thread::Builder::new().stack_size(16 << 20).spawn(serve) {
        Ok(worker) => match worker.join() {
            Ok(Ok(())) => ExitCode::SUCCESS,
            Ok(Err(error)) => {
                eprintln!("replay_worker: {error}");
                ExitCode::FAILURE
            }
            Err(_) => ExitCode::from(101),
        },
        Err(error) => {
            eprintln!("replay_worker: {error}");
            ExitCode::FAILURE
        }
    }
}
