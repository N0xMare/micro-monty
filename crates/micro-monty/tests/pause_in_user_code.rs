//! Pause-anywhere through user code that builtins and operations reach
//! (docs/spec.md §5, §9.1): a host call and a requested park inside a
//! `key=` function, a user `__eq__` during a dict lookup, `__repr__` during
//! `print([obj])`, `__init__`, and a generator consumed by `sum()` each
//! surface to the facade and resume to the same result. None of them may
//! run on a nested dispatch loop.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{Engine, EngineConfig, HostModule, HostValue, Progress, Resume, Session};
use std::sync::{Arc, Mutex};

/// Shared by every scenario: `spin` gives the requested park safepoints
/// inside the user code that called `probe`.
const PRELUDE: &str = "\
def spin():
    for _ in range(3000):
        pass
";

/// Runs `body`; every `probe(n)` host call answers `n * 10`. After the
/// first host call a park is requested, so the cell must park inside the
/// same user code (before anything is printed) and then finish with
/// `expected` as its output.
fn check(what: &str, body: &str, expected: &str, min_calls: usize) {
    let out = Arc::new(Mutex::new(String::new()));
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).expect("session");
    let sink = Arc::clone(&out);
    s.set_output(Box::new(move |t: &str| {
        sink.lock().expect("output lock").push_str(t);
    }));
    s.define(&HostModule::untyped("", &["probe"]))
        .expect("defined");
    let code = engine
        .compile(&format!("{PRELUDE}{body}"), "pause.py")
        .expect("compiles");
    let mut p = s.start(&code);
    let (mut calls, mut parks) = (0, 0);
    loop {
        match p {
            Progress::HostCall { name, args, .. } => {
                assert_eq!(name, "probe", "{what}");
                let [HostValue::Int(n)] = args.as_slice() else {
                    panic!("{what}: probe args {args:?}");
                };
                calls += 1;
                if calls == 1 {
                    s.request_park();
                }
                p = s
                    .resume(Resume::Value(HostValue::Int(n * 10)))
                    .expect("resumes");
            }
            Progress::Parked { .. } => {
                parks += 1;
                assert_eq!(calls, 1, "{what}: parked after the first host call");
                assert_eq!(
                    out.lock().expect("output lock").as_str(),
                    "",
                    "{what}: parked before any output"
                );
                p = s.resume(Resume::Continue).expect("continues");
            }
            Progress::Complete(_) => break,
            other => panic!("{what}: unexpected {other:?}"),
        }
    }
    assert!(calls >= min_calls, "{what}: {calls} host calls");
    assert_eq!(parks, 1, "{what}: parks");
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        expected,
        "{what}"
    );
}

#[test]
fn inside_a_sort_key_function() {
    check(
        "key=",
        "\
def key(v):
    r = probe(v)
    spin()
    return -r
print(sorted([1, 3, 2], key=key), max([5, 7, 6], key=key))
",
        "[3, 2, 1] 5\n",
        6,
    );
}

#[test]
fn inside_eq_during_a_dict_lookup() {
    check(
        "__eq__ in a dict probe",
        "\
class K:
    def __init__(self, v):
        self.v = v
    def __hash__(self):
        return 1
    def __eq__(self, o):
        probe(self.v)
        spin()
        return self.v == o.v
d = {K(1): 'one', K(2): 'two'}
print(d[K(2)], K(3) in d)
",
        "two False\n",
        3,
    );
}

#[test]
fn inside_repr_while_printing_a_list() {
    check(
        "__repr__ in print([obj])",
        "\
class R:
    def __init__(self, v):
        self.v = v
    def __repr__(self):
        r = probe(self.v)
        spin()
        return f'R{r}'
print([R(1), (R(2),)], {'k': R(3)})
",
        "[R10, (R20,)] {'k': R30}\n",
        3,
    );
}

#[test]
fn inside_init() {
    check(
        "__init__",
        "\
class P:
    def __init__(self, v):
        self.v = probe(v)
        spin()
class Q(P):
    def __init__(self, v):
        super().__init__(v + 1)
print(P(4).v, Q(4).v)
",
        "40 50\n",
        2,
    );
}

#[test]
fn inside_a_generator_consumed_by_sum() {
    check(
        "generator in sum()",
        "\
def g():
    for i in range(4):
        yield probe(i)
        spin()
print(sum(g()), sum(x for x in g()))
",
        "60 60\n",
        8,
    );
}
