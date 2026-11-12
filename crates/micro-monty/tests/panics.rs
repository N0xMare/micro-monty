//! Panic containment (docs/spec.md §10.3): with unwinding, an engine panic
//! inside a facade operation poisons only that session; the host and every
//! other session keep working. Panics are injected with the `test-hooks`
//! feature, which this crate's tests enable.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    Engine, EngineConfig, HostModule, HostValue, Progress, Resume, Session, SnapshotError,
    test_hooks,
};

fn engine() -> Engine {
    Engine::new(EngineConfig::default())
}

/// A session stopped at a host call of `f`, with `x` assigned.
fn stopped(engine: &Engine) -> Session {
    let mut session = Session::new(engine).expect("session");
    session
        .define(&HostModule::untyped("", &["f"]))
        .expect("host function");
    let code = engine
        .compile("x = 41\ny = f(1)\nx + y", "t.py")
        .expect("compiles");
    assert!(matches!(session.start(&code), Progress::HostCall { .. }));
    session
}

fn is_internal_error(progress: &Progress) -> bool {
    matches!(progress, Progress::Error(e) if e.type_name == "SystemError"
        && e.message.contains("injected panic")
        && e.message.contains("poisoned"))
}

/// Every operation of a poisoned session fails with a typed error and
/// never reaches the heap again.
fn assert_poisoned(engine: &Engine, session: &mut Session) {
    assert!(session.is_poisoned());
    let code = engine.compile("1", "t.py").expect("compiles");
    assert!(is_internal_error(&session.start(&code)));
    assert!(is_internal_error(
        &session.resume(Resume::Continue).expect("typed error")
    ));
    assert!(is_internal_error(
        &session
            .resume(Resume::Value(HostValue::Int(1)))
            .expect("typed error")
    ));
    assert_eq!(session.snapshot(), Err(SnapshotError::Poisoned));
    assert_eq!(
        session.reset().expect_err("poisoned").type_name,
        "SystemError"
    );
    assert!(!session.cancel());
    assert!(session.define(&HostModule::untyped("", &["g"])).is_err());
    assert!(session.define(&HostModule::untyped("m", &["h"])).is_err());
}

/// A session that never panicked still runs, snapshots and restores.
fn assert_healthy(engine: &Engine) {
    let mut other = stopped(engine);
    let saved = other.snapshot().expect("snapshot");
    let mut restored = engine.restore(&saved).expect("restore");
    for session in [&mut other, &mut restored] {
        match session.resume(Resume::Value(HostValue::Int(1))) {
            Ok(Progress::Complete(Some(c))) => assert_eq!(c.repr, "42"),
            other => panic!("{other:?}"),
        }
        assert!(!session.is_poisoned());
    }
}

#[test]
fn a_panic_in_any_session_operation_poisons_only_that_session() {
    let engine = engine();
    // Keep the injected panics' messages off the test output.
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for op in ["start", "resume", "snapshot", "reset", "cancel"] {
        // A healthy neighbour, mid-cell when the panic hits.
        let mut neighbour = stopped(&engine);
        let mut session = if op == "start" {
            Session::new(&engine).expect("session")
        } else {
            stopped(&engine)
        };
        test_hooks::panic_next(op);
        match op {
            "start" => {
                let code = engine.compile("x = [1, 2]\nx", "t.py").expect("compiles");
                assert!(is_internal_error(&session.start(&code)), "{op}");
            }
            "resume" => {
                let progress = session
                    .resume(Resume::Value(HostValue::Int(1)))
                    .expect("typed error");
                assert!(is_internal_error(&progress), "{op}");
            }
            "snapshot" => assert_eq!(session.snapshot(), Err(SnapshotError::Poisoned)),
            "reset" => assert!(session.reset().is_err()),
            "cancel" => assert!(!session.cancel()),
            _ => unreachable!(),
        }
        assert_poisoned(&engine, &mut session);
        match neighbour.resume(Resume::Value(HostValue::Int(1))) {
            Ok(Progress::Complete(Some(c))) => assert_eq!(c.repr, "42", "{op}"),
            other => panic!("{op}: {other:?}"),
        }
        assert_healthy(&engine);
    }
    std::panic::set_hook(quiet);
}

#[test]
fn a_panic_in_compile_or_restore_is_a_typed_error() {
    let engine = engine();
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    test_hooks::panic_next("compile");
    let error = engine.compile("1 + 1", "t.py").expect_err("panicked");
    assert_eq!(error.type_name, "SystemError");
    assert!(error.message.contains("injected panic"), "{error:?}");
    engine.compile("1 + 1", "t.py").expect("compiles again");

    let saved = stopped(&engine).snapshot().expect("snapshot");
    test_hooks::panic_next("restore");
    let error = engine.restore(&saved).err();
    assert!(
        matches!(&error, Some(SnapshotError::Internal { message }) if message.contains("injected panic")),
        "{error:?}"
    );
    std::panic::set_hook(quiet);
    assert_healthy(&engine);
}
