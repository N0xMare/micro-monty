//! What the facade's module features keep and refuse: coroutines, async
//! generators, `async with` and awaited host futures work without the
//! `asyncio` module, and a module whose feature is off raises
//! `ModuleNotFoundError`, while `math`, `cmath` and `sys` have no feature
//! and always import. `cargo xtask check` also runs this file on a build
//! that opts out of `asyncio` and `json` and into `ast` (`cargo test -p
//! micro-monty --no-default-features --features mod-ast`); the
//! workspace build has the default set, and `ast` and `uuid` through mm-cli.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{Engine, EngineConfig, HostModule, HostValue, Progress, Resume, Session};

fn run_to_end(engine: &Engine, s: &mut Session, src: &str) -> String {
    let code = engine.compile(src, "test.py").expect("compiles");
    let mut p = s.start(&code);
    loop {
        p = match p {
            Progress::HostCall { call, args, .. } => {
                let HostValue::Int(k) = args[0] else {
                    panic!("{args:?}")
                };
                let p = s.resume(Resume::Future).expect("resumes");
                assert_eq!(p, Progress::Waiting { calls: vec![call] });
                s.resume(Resume::Batch(vec![(call, Ok(HostValue::Int(k * 10)))]))
                    .expect("resumes")
            }
            Progress::Complete(Some(c)) => return c.repr,
            other => panic!("{other:?}"),
        };
    }
}

#[test]
fn coroutines_and_host_futures_need_no_asyncio_module() {
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).expect("session");
    s.define(&HostModule::untyped("tools", &["fetch"]))
        .expect("defined");
    let src = "\
from tools import fetch
async def double(x):
    return 2 * x
def finish(c):
    try:
        c.send(None)
    except StopIteration as e:
        return e.value
async def agen():
    yield 1
    yield 2
async def collect():
    return [v async for v in agen()]
class CM:
    async def __aenter__(self):
        return 'in'
    async def __aexit__(self, *exc):
        return False
async def inside():
    async with CM() as v:
        return v
c = double(5)
c.close()
f = fetch(4)
r = repr(f)
[finish(double(4)), finish(collect()), finish(inside()), await f, await double(await fetch(3)), r]
";
    assert_eq!(
        run_to_end(&engine, &mut s, src),
        "[8, [1, 2], 'in', 40, 60, '<Future pending>']"
    );
}

#[test]
fn a_module_imports_exactly_when_its_feature_is_on() {
    let engine = Engine::new(EngineConfig::default());
    let env = engine.environment();
    // No other module feature turns these on.
    for (module, on) in [
        ("asyncio", cfg!(feature = "mod-asyncio")),
        ("asyncio.tasks", cfg!(feature = "mod-asyncio")),
        ("json", cfg!(feature = "mod-json")),
        ("ast", cfg!(feature = "mod-ast")),
        ("uuid", cfg!(feature = "mod-uuid")),
        // Engine modules with no feature, in every build.
        ("math", true),
        ("cmath", true),
        ("sys", true),
    ] {
        let mut s = Session::new(&engine).expect("session");
        let src = format!(
            "try:\n    import {module}\n    r = 'imported'\nexcept ModuleNotFoundError as e:\n    \
             r = str(e)\nr"
        );
        let got = run_to_end(&engine, &mut s, &src);
        let top = module.split('.').next().expect("name");
        if on {
            assert_eq!(got, "'imported'", "{module}");
        } else {
            assert!(
                got.starts_with(&format!("\"No module named '{top}'")),
                "{module}: {got}"
            );
            assert!(env.contains("Not enabled in this build: "), "{env}");
        }
    }
}
