//! Sessions move between threads: a tokio embedder parks a cell on one
//! worker thread and resumes it on another. `Session` is `Send`, not
//! `Sync`; `Engine` and `Code` are both, so one engine serves many tasks.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{Engine, EngineConfig, HostModule, HostValue, Progress, Resume, Session};
use std::sync::{Arc, Mutex};

const fn is_send<T: Send>() {}
const fn is_sync<T: Sync>() {}

// Checked at compile time: this file does not build if any bound breaks.
const _: () = {
    is_send::<Engine>();
    is_sync::<Engine>();
    is_send::<micro_monty::Code>();
    is_sync::<micro_monty::Code>();
    is_send::<Session>();
    is_send::<Progress>();
    is_send::<micro_monty::ParkHandle>();
};

#[test]
fn a_session_parked_on_one_thread_resumes_on_another() {
    let engine = Engine::new(EngineConfig::default());
    let code = engine
        .compile(
            "total = 0\nfor i in range(3):\n    total += ask(i)\nprint(total)\n",
            "<cell>",
        )
        .expect("compiles");
    let out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&out);
    let mut session = Session::new(&engine).expect("session");
    session
        .define(&HostModule::untyped("", &["ask"]))
        .expect("defined");
    session.set_output(Box::new(move |s: &str| {
        sink.lock().expect("output lock").push_str(s);
    }));

    let mut progress = session.start(&code);
    while let Progress::HostCall { args, .. } = progress {
        let HostValue::Int(i) = args[0] else {
            panic!("int argument, got {args:?}");
        };
        // Each answer is given on a fresh thread that owns the session.
        (session, progress) = std::thread::spawn(move || {
            let p = session
                .resume(Resume::Value(HostValue::Int(i * 10)))
                .expect("resumes");
            (session, p)
        })
        .join()
        .expect("thread");
    }
    assert!(matches!(progress, Progress::Complete(_)), "{progress:?}");
    assert_eq!(out.lock().expect("output lock").as_str(), "30\n");
}
