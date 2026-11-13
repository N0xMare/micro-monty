//! Facade behaviour: completion values, output, limits, and inputs nested
//! deeply enough to overflow a naively recursive engine.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    Engine, EngineConfig, HostModule, Limits, ParkReason, Progress, Resume, Session,
};
use std::{
    fmt::Write as _,
    sync::{Arc, Mutex},
};

fn run_with(limits: Limits, src: &str) -> (Progress, String) {
    let engine = Engine::new(EngineConfig::new().with_limits(limits));
    let code = engine.compile(src, "test.py").expect("compiles");
    let mut session = Session::new(&engine).expect("session");
    let out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&out);
    session.set_output(Box::new(move |s: &str| {
        sink.lock().expect("output lock").push_str(s);
    }));
    let progress = session.start(&code);
    let text = out.lock().expect("output lock").clone();
    (progress, text)
}

fn run(src: &str) -> (Progress, String) {
    run_with(Limits::default(), src)
}

fn error_of(p: &Progress) -> &micro_monty::PyException {
    match p {
        Progress::Error(e) => e,
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn completes_with_the_last_expression() {
    let (p, out) = run("print('hi', end='')\n[1, 'a', None]");
    assert_eq!(out, "hi");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        (c.repr.as_str(), c.str.as_str(), c.type_name.as_str()),
        ("[1, 'a', None]", "[1, 'a', None]", "list")
    );
    let (p, _) = run("x = 1");
    assert_eq!(p, Progress::Complete(None));
}

#[test]
fn completion_text_is_cut_at_one_mib_by_default() {
    // The default text limit was the memory limit (64 MiB): a 2.4 MB
    // result reached the host whole. It is now 1 MiB (owner decision);
    // the value stays whole and a host can raise the limit.
    let src = "'\u{e9}' * 1_200_000";
    let (p, _) = run(src);
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert!(c.repr_truncated && c.str_truncated);
    // Cut at a character boundary: after the quote, two-byte chars.
    assert_eq!((c.repr.len(), c.str.len()), (1_048_575, 1_048_576));
    assert!(c.repr.starts_with("'\u{e9}\u{e9}") && c.str.ends_with('\u{e9}'));
    let Some(micro_monty::HostValue::Str(whole)) = &c.value else {
        panic!("{:?}", c.value)
    };
    assert_eq!(whole.len(), 2_400_000);
    let engine = Engine::new(EngineConfig::default());
    let code = engine.compile(src, "big.py").expect("compiles");
    let mut session = Session::new(&engine).expect("session");
    session.set_text_limit(4 << 20);
    let Progress::Complete(Some(c)) = session.start(&code) else {
        panic!("completes")
    };
    assert!(!c.repr_truncated && !c.str_truncated);
    assert_eq!((c.repr.len(), c.str.len()), (2_400_002, 2_400_000));
}

#[test]
fn ten_thousand_nested_parentheses_is_a_syntax_error() {
    let src = format!("x = {}1{}\n", "(".repeat(10_000), ")".repeat(10_000));
    let engine = Engine::new(EngineConfig::default());
    let err = engine.compile(&src, "deep.py").expect_err("too deep");
    assert_eq!(err.type_name, "SyntaxError");
    assert_eq!(err.message, "too many nested parentheses");
    assert!(
        err.rendered
            .ends_with("SyntaxError: too many nested parentheses\n")
    );
}

#[test]
fn lone_carriage_returns_end_lines_in_errors_and_tracebacks() {
    // As `python3.14 r.py` prints them.
    let engine = Engine::new(EngineConfig::default());
    let e = engine
        .compile("x = 1\ry = = 2\r", "r.py")
        .expect_err("syntax");
    assert_eq!(
        e.rendered,
        "  File \"r.py\", line 2\n    y = = 2\n        ^\nSyntaxError: invalid syntax\n"
    );
    let code = engine
        .compile("def f():\r    1/0\r\nf()\r", "r.py")
        .expect("compiles");
    let mut session = Session::new(&engine).expect("session");
    let Progress::Error(e) = session.start(&code) else {
        panic!("raises")
    };
    assert_eq!(
        e.traceback,
        "Traceback (most recent call last):\n  File \"r.py\", line 3, in <module>\n    f()\n    \
         ~^^\n  File \"r.py\", line 2, in f\n    1/0\n    ~^~\nZeroDivisionError: division by zero\n"
    );
}

#[test]
fn null_bytes_in_source_are_a_syntax_error() {
    // CPython 3.14 reads the file line by line: the line holding the null
    // byte fails before any later error and every parser error, after a
    // tokenizer error on an earlier line; the text stops at the null byte
    // and has no caret. mm accepted a null byte in a comment.
    let engine = Engine::new(EngineConfig::default());
    for (src, want) in [
        ("# a\0b\nprint(1)\n", "  File \"n.py\", line 1\n    # a\n"),
        (
            "print(1)\nx = \"a\0b\"\n",
            "  File \"n.py\", line 2\n    x = \"a\n",
        ),
        ("\0", "  File \"n.py\", line 1\n    \n"),
        ("x = ) \0\n", "  File \"n.py\", line 1\n    x = ) \n"),
        (
            "def f(:\n  pass\nx = 1\n\0",
            "  File \"n.py\", line 4\n    \n",
        ),
    ] {
        let e = engine.compile(src, "n.py").expect_err("null byte");
        assert_eq!(
            e.rendered,
            format!("{want}SyntaxError: source code cannot contain null bytes\n"),
            "{src:?}"
        );
        assert_eq!(e.type_name, "SyntaxError");
    }
    // A tokenizer error on an earlier line is reported first.
    let e = engine
        .compile("x = )\ny\0\n", "n.py")
        .expect_err("unmatched");
    assert_eq!(e.message, "unmatched ')'");
}

#[test]
fn deep_operator_chains_are_bounded() {
    let engine = Engine::new(EngineConfig::default());
    // A flat chain is one level of nesting: 60,000 terms compile (CPython's
    // compiler overflows its stack between 12,000 and 50,000).
    let flat = format!("x = 1{}\n", " + 1".repeat(60_000));
    assert!(engine.compile(&flat, "flat.py").is_ok());
    for src in [
        format!("x = {}1\n", "-".repeat(50_000)),
        format!("x = {}1\n", "not ".repeat(50_000)),
        format!("x = 1{}\n", "+1".repeat(100_001)),
        format!("f = {}1\n", "lambda: ".repeat(20_000)),
    ] {
        let err = engine.compile(&src, "deep.py").expect_err("too deep");
        assert!(
            matches!(err.type_name.as_str(), "SyntaxError" | "RecursionError"),
            "{err}"
        );
    }
}

#[test]
fn hundred_thousand_deep_containers_raise_recursion_error() {
    let (p, out) = run(
        "x = []\nfor i in range(100000):\n    x = [x]\ntry:\n    repr(x)\nexcept RecursionError as e:\n    print('repr', e)\n\
         y = []\nfor i in range(100000):\n    y = [y]\ntry:\n    x == y\nexcept RecursionError as e:\n    print('eq', e)\n\
         t = ()\nfor i in range(100000):\n    t = (t,)\ntry:\n    hash(t)\nexcept RecursionError:\n    print('hash')\n\
         str(x)\n",
    );
    assert_eq!(
        out,
        "repr maximum recursion depth exceeded while getting the repr of an object\n\
         eq maximum recursion depth exceeded in comparison\nhash\n"
    );
    let e = error_of(&p);
    assert_eq!(e.type_name, "RecursionError");
    // A 20k-deep list is fine, as on CPython.
    let (p, out) = run("z = []\nfor i in range(20000):\n    z = [z]\nprint(len(repr(z)))");
    assert_eq!(out, "40002\n");
    assert!(matches!(p, Progress::Complete(_)));
    // json.loads takes 100,000 levels and raises past them (CPython's limit
    // is its C stack, so its message names a stack overflow instead).
    let (p, out) = run(
        "import json\nfor n in (100000, 100001):\n    for o, c in (('[', ']'), ('{\"a\":', '}')):\n        \
         try:\n            json.loads(o * n + '1' + c * n)\n            print(n, 'ok')\n        \
         except RecursionError as e:\n            print(n, e)\n",
    );
    assert_eq!(
        out,
        "100000 ok\n100000 ok\n\
         100001 maximum recursion depth exceeded while decoding a JSON array from a unicode string\n\
         100001 maximum recursion depth exceeded while decoding a JSON object from a unicode string\n"
    );
    assert!(matches!(p, Progress::Complete(_)));
}

#[test]
fn deep_class_tuples_and_builtin_chains_stay_off_the_rust_stack() {
    // isinstance/issubclass used to recurse per tuple level and overflow
    // the Rust stack; `object.__str__` as `__repr__` chained builtins
    // without end (each hop re-pushing every pending continuation).
    let (p, out) = run("t = (int,)\nfor _ in range(200000):\n    t = (t,)\n\
         print(isinstance(1, t), isinstance('a', t), issubclass(bool, t))\n\
         class Foo:\n    pass\nFoo.__repr__ = Foo.__str__\n\
         try:\n    repr(Foo())\nexcept RecursionError as e:\n    print(e)\n");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out,
        "True False True\nmaximum recursion depth exceeded while calling a Python object\n"
    );
}

#[test]
fn deep_frozensets_and_dict_keys_raise_recursion_error() {
    // Comparisons nested through set and dict probes count towards the
    // depth limit (they used to restart it and overflow the Rust stack).
    let src = "\
u, v = 'u', 'u'
for i in range(50000):
    u = frozenset([u, 2])
    v = frozenset([v, 2])
for f in (lambda: u == v, lambda: {u: 1} == {v: 1}, lambda: u in {v}, lambda: [u] < [v]):
    try:
        f()
    except RecursionError as e:
        print(e)
";
    let out = on_small_stack(move || run(src).1);
    assert_eq!(
        out,
        "maximum recursion depth exceeded in comparison\n".repeat(4)
    );
}

/// Runs `f` on a 256 KiB thread, the smallest host stack docs/user-guide.md
/// promises: comparison walks keep their nesting in the arena, not on the
/// Rust stack.
fn on_small_stack<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(f)
        .expect("spawns")
        .join()
        .expect("joins")
}

#[test]
fn deep_and_cyclic_comparisons_fit_a_small_host_stack() {
    // A self-referencing dict and a 600-deep frozenset chain used to recurse
    // natively up to the depth limit: both aborted on 256 KiB and 512 KiB
    // threads, and under wasmtime's default stack.
    let src = "\
a = {}
a['x'] = a
b = {}
b['x'] = b
try:
    a == b
except RecursionError as e:
    print(e)
u, v = 'u', 'u'
for i in range(600):
    u = frozenset([u, 2])
    v = frozenset([v, 2])
print(u == v, u <= v, {u: 1} == {v: 1}, [u] < [v], u in {v})
x = y = 0
for i in range(300):
    x, y = [0, {1: (x,)}], [0, {1: (y,)}]
print(x == y, x <= y, {1: x}.items() == {1: y}.items())
";
    let (p, out) = on_small_stack(move || run(src));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out,
        "maximum recursion depth exceeded in comparison\n\
         True True True False True\nTrue True True\n"
    );
}

#[test]
fn comparing_cyclic_lists_of_deep_frozensets_raises_promptly() {
    // libFuzzer nested_data F1. Each native walk ran out of its budget
    // before the depth limit, and every level the prelude then took over
    // restarted it at depth 0 with a full budget: up to ten times the
    // recursion limit in prelude levels, two full walks each (about 333M
    // fuel and a minute in release for one `<`). The nesting now carries
    // across those levels, so each comparison raises within a few million
    // fuel units.
    let src = "\
def build():
    x = 'leaf'
    for _ in range(16):
        x = frozenset([x, 2])
    c = [x]
    c.append(c)
    return c
a, b = build(), build()
for f in (lambda: b < a, lambda: a == b, lambda: max([b, a]), lambda: {'k': a} == {'k': b}):
    try:
        f()
    except RecursionError as e:
        print(e)
";
    let (p, out, elapsed) = on_small_stack(move || {
        let engine = Engine::new(EngineConfig::default());
        let code = engine.compile(src, "cyclic.py").expect("compiles");
        let mut session = Session::new(&engine).expect("session");
        let out = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&out);
        session.set_output(Box::new(move |s: &str| {
            sink.lock().expect("output lock").push_str(s);
        }));
        session.set_fuel(Some(20_000_000));
        let t = std::time::Instant::now();
        let p = session.start(&code);
        let text = out.lock().expect("output lock").clone();
        (p, text, t.elapsed())
    });
    assert_eq!(p, Progress::Complete(None));
    assert_eq!(
        out,
        "maximum recursion depth exceeded in comparison\n".repeat(4)
    );
    assert!(elapsed.as_secs() < 60, "took {elapsed:?}");
}

#[test]
fn comparing_long_runs_of_identical_items_spends_fuel() {
    // Identical items (shared ints here) were passed over without spending
    // the walk's budget, natively and in the prelude's `_seq_scan`: these
    // 300 scans of a million items cost about 127k fuel and ran to the
    // end under this limit. Each now costs a visit per 64 items.
    let src = "\
class E:
    def __eq__(self, other):
        return True
a = [0] * 10**6
b = a[:]
b[-1] = 1
c = [E()] + a
d = [E()] + b
n = 0
for i in range(100):
    n += (a == b) + (a < b) + (c != d)
n
";
    let limits = Limits::new().with_fuel(Some(1_000_000));
    let engine = Engine::new(EngineConfig::new().with_limits(limits));
    let code = engine.compile(src, "same.py").expect("compiles");
    let mut session = Session::new(&engine).expect("session");
    let p = session.start(&code);
    assert_eq!(
        p,
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    );
    session.set_fuel(None);
    let p = session.resume(Resume::Continue).expect("resumes");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(c.repr, "200");
}

#[test]
fn python_recursion_is_limited_without_rust_recursion() {
    let (p, _) = run("def f(n):\n    return f(n + 1)\nf(0)\n");
    let e = error_of(&p);
    assert_eq!(e.type_name, "RecursionError");
    assert!(
        e.traceback
            .contains("[Previous line repeated 996 more times]"),
        "{}",
        e.traceback
    );
    let limits = Limits::new().with_recursion_depth(50);
    let (p, out) = run_with(
        limits,
        "def d(n):\n    return 0 if n == 0 else 1 + d(n - 1)\nprint(d(40))\nd(60)\n",
    );
    assert_eq!(out, "40\n");
    assert_eq!(error_of(&p).type_name, "RecursionError");

    // Class-valued initializers need the same protection at the SDK's
    // default depth, not only at the CLI's smaller conformance limit.
    let (p, _) = run(include_str!(
        "../../../conformance/monty/recursion__instance_repr.py"
    ));
    assert_eq!(p, Progress::Complete(None));
}

/// An exhausted builtin iterator drops its sequence, as python3.14's do:
/// three exhausted iterators over 8 MB lists kept the lists, so the next
/// three raised `MemoryError` under a 32 MiB limit. Each kind of iterator
/// that holds a sequence, `reversed` included.
#[test]
fn exhausted_iterators_drop_their_sequences() {
    for make in [
        "iter([0] * 1_000_000)",
        "reversed([0] * 1_000_000)",
        "iter(tuple([0] * 1_000_000))",
        "iter(bytearray(8_000_000))",
        "iter(dict.fromkeys(range(250_000)))",
        "iter(set(range(250_000)))",
    ] {
        let src = format!(
            "its = []\nfor _ in range(3):\n    it = {make}\n    min(it)\n    its.append(it)\n\
             it = None\nkeep = [[0] * 1_000_000 for _ in range(3)]\nprint(len(keep))\n"
        );
        let (p, out) = run_with(Limits::new().with_memory(32 << 20), &src);
        assert!(matches!(p, Progress::Complete(_)), "{make}: {p:?}");
        assert_eq!(out, "3\n", "{make}");
    }
}

/// Small native charges add up to a safepoint check: a straight-line
/// body of 300 slices of a 3,840-char str (59 fuel units each, under the
/// 64 of one long charge) reached the collection trigger only once per 256
/// iterations, 256 x 300 slices of garbage between them, and raised
/// `MemoryError` at the default 256 MiB; python3.14 prints `900 3540`.
#[test]
fn small_native_charges_reach_the_collection_trigger() {
    let mut src = String::from("s = 'x' * 3840\nn = 0\nwhile n < 900:\n");
    for i in 1..=300 {
        writeln!(src, "    x = s[{i}:]").expect("write");
    }
    src.push_str("    n += 1\nprint(n, len(x))\n");
    let (p, out) = run(&src);
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(out, "900 3540\n");
}

/// Garbage not yet collected counts against the limit, so an instruction
/// whose native needs a large block beside it raised `MemoryError` where a
/// collection would have made room, depending on where collections fell
/// (`import time` moves them): `ba += x` growing past its old growth
/// blocks, `bytes +=`, an f-string, a list slice store. Each now runs
/// again once a collection has ended, as a builtin call does
/// (`Vm::oom_retry_op`), and only once: the bytearray's length shows the
/// retried `+=` applied once. A user `__add__` that raises `MemoryError`
/// is not called again. python3.14 prints the same.
#[test]
fn an_instruction_out_of_memory_waits_for_a_collection_once() {
    const KEEP: &str = "keep = [bytearray(1_000_000) for _ in range(60)]\n";
    for (body, want) in [
        (
            "xs = [bytearray(b'a' * 1_000_000) for _ in range(60)]\nba = bytearray()\n\
             for x in xs:\n    ba += x\nr = len(ba)",
            "60000000",
        ),
        (
            "x = b'a' * 1_000_000\ns = b''\nfor _ in range(60):\n    s += x\nr = len(s)",
            "60000000",
        ),
        (
            "x = 'a' * 1_000_000\ns = ''\nfor _ in range(60):\n    s = f'{s}{x}'\nr = len(s)",
            "60000000",
        ),
        (
            "l = [0] * 7_000_000\nfor i in range(0, 7_000_000, 100_000):\n    l[i:i + 1] = [1, 2]\nr = len(l)",
            "7000070",
        ),
        // A starred display's tuple (`LIST_TO_TUPLE`) and a starred target
        // (`UNPACK_EX`).
        (
            "l = list(range(2_000_000))\nfor _ in range(40):\n    t = (*l, *l)\nr = len(t)",
            "4000000",
        ),
        (
            "l = list(range(2_000_000))\nfor _ in range(40):\n    a, *m = l\nr = len(m)",
            "1999999",
        ),
        // A starred display's `*l` (`LIST_EXTEND`, which grows its list once
        // before writing) and a starred call of a function (`CALL_EX`, whose
        // `*args` tuple is built before the frame is entered).
        (
            "l = list(range(2_500_000))\nfor _ in range(40):\n    t = (*l, *l)\nr = len(t)",
            "5000000",
        ),
        (
            "c = [0] * 1_000_000\nt = ()\ndef f(*a): return a\n\
             for _ in range(7):\n    t = f(*t, *c)\nr = len(t)",
            "7000000",
        ),
        (
            "t = ()\nc = (0,) * 1_000_000\nfor _ in range(7):\n    t = (*t, *c)\nr = len(t)",
            "7000000",
        ),
        // A starred target of a starred display: a collection during the
        // display's long `*l` (a rebuild in steps) found the old buffer in
        // use, so the retry saw nothing allocated since and gave up.
        (
            "c = list(range(1_000_000))\nl = []\nfor _ in range(7):\n    a, *l = [0, *l, *c]\nr = len(l)",
            "7000000",
        ),
        (
            "n = 0\nclass A:\n    def __add__(self, o):\n        global n\n        n += 1\n        raise MemoryError\n\
             try:\n    A() + 1\nexcept MemoryError:\n    pass\nr = n",
            "1",
        ),
    ] {
        let keep = if body.starts_with("xs") { "" } else { KEEP };
        let (p, out) = run(&format!("import time\n{keep}{body}\nprint(r)\n"));
        assert!(matches!(p, Progress::Complete(_)), "{body}: {p:?}");
        assert_eq!(out, format!("{want}\n"), "{body}");
    }
}

/// An instruction or call that drains a one-shot iterator and then runs out
/// of memory does not run again after a collection: the rerun would gather
/// what is left, so `l[:] = zip(a, b)` once emptied `l`, with no
/// exception, at pads 16,908,288-17,367,040 [M]. Swept across that window:
/// each either assigns all 60,000 items as python3.14 does or raises
/// `MemoryError` with `l` unchanged.
#[test]
fn a_rerun_never_gathers_a_drained_iterator_again() {
    for pad in (16_842_752..17_498_113).step_by(1 << 17) {
        let src = format!(
            "import gc\nsrc = list(range(60000))\nkeep = [bytearray(1 << 20) for _ in range(236)]\n\
             pad = bytearray({pad})\nit = zip(src, src)\nl = ['old']\ngc.collect()\n\
             try:\n    l[:] = it\n    print('assigned', len(l), len(list(it)))\n\
             except MemoryError:\n    print('MemoryError', l)\n"
        );
        let (p, out) = run(&src);
        assert!(matches!(p, Progress::Complete(_)), "{pad}: {p:?}");
        assert!(
            out == "assigned 60000 0\n" || out == "MemoryError ['old']\n",
            "slice store, pad {pad}: {out}"
        );
    }
}

/// `+=` and the other in-place operators on an instance of a list, dict or
/// set subclass change its backing object before they can fail, as on the
/// builtin type, so they do not run again after a collection: `x += c` on
/// a list subclass once appended `c`'s first items twice (`ok False 8` at
/// pads 12,058,624-14,417,920 [M]). Swept across that window.
#[test]
fn an_in_place_op_on_a_subclass_is_not_rerun() {
    for pad in (12_058_624..14_680_000).step_by(1 << 19) {
        let src = format!(
            "import gc\nclass L(list): pass\nx = L()\nfor i in range((1 << 20) - 3): x.append(i)\n\
             keep = [bytearray(1 << 20) for _ in range(216)]\npad = bytearray({pad})\n\
             c = [-1, -2, -3, -4, -5]\ngc.collect()\nt = [0] * 300000\nt = None\nn0 = len(x)\n\
             try:\n    x += c\n    r = 'ok'\nexcept MemoryError:\n    r = 'ME'\n\
             print(r, x[n0:] == c[:len(x) - n0], len(x) - n0)\n"
        );
        let (p, out) = run(&src);
        assert!(matches!(p, Progress::Complete(_)), "{pad}: {p:?}");
        // All of `c` once, or nothing (a list grows before it changes).
        assert!(
            out == "ok True 5\n" || out == "ME True 0\n",
            "pad {pad}: {out}"
        );
    }
}

/// A list's `+=` and `*=` run again after a collection only when nothing
/// ran that the rerun would repeat: a user `__iter__`, a generator and an
/// `__index__` (and a `__rmul__` before it) are each called once around a
/// `MemoryError`, swept across the pads where a retry of the native op
/// turns `ME` into `ok` [M]. The operand's own iteration fails as
/// `CPython`'s does, keeping what it appended.
#[test]
fn a_retried_list_op_never_calls_user_code_again() {
    for (kind, make, op, pads) in [
        (
            "iter",
            "class S(list):\n    def __iter__(self):\n        global n\n        n += 1\n        return iter(c)\ny = S()",
            "x += y",
            (13_631_488..15_204_353).step_by(1 << 18),
        ),
        (
            "gen",
            "def g():\n    global n\n    n += 1\n    yield from c\ny = None",
            "x += g()",
            (13_631_488..15_204_353).step_by(1 << 18),
        ),
        (
            "index",
            "class R:\n    def __rmul__(self, o):\n        global n\n        n += 100\n        return NotImplemented\n\
             \x20   def __index__(self):\n        global n\n        n += 1\n        return 2\ny = R()",
            "x *= y",
            (16_056_320..16_449_537).step_by(1 << 16),
        ),
    ] {
        for pad in pads {
            let src = format!(
                "import gc\nn = 0\nx = []\nfor i in range((1 << 20) - 3): x.append(i)\n\
                 c = [-1, -2, -3, -4, -5]\n{make}\nkeep = [bytearray(1 << 20) for _ in range(216)]\n\
                 pad = bytearray({pad})\ngc.collect()\nt = [0] * 300000\nt = None\nn0 = len(x)\n\
                 try:\n    {op}\n    r = 'ok'\nexcept MemoryError:\n    r = 'ME'\n\
                 tail = x[n0:n0 + 5]\n\
                 good = x[n0 - 1] == n0 - 1 and tail in (c[:len(tail)], list(range(len(tail))))\n\
                 print(r, n, len(x) - n0 if good else 'changed')\n"
            );
            let (p, out) = run(&src);
            assert!(matches!(p, Progress::Complete(_)), "{kind} {pad}: {p:?}");
            let (calls, full) = if kind == "index" {
                ("101", "1048573")
            } else {
                ("1", "5")
            };
            // All of it, nothing, or (iterating the operand) a prefix of it.
            let ok = [format!("ok {calls} {full}\n"), format!("ME {calls} 0\n")];
            let prefix = kind != "index" && (1..5).any(|k| out == format!("ME {calls} {k}\n"));
            assert!(ok.contains(&out) || prefix, "{kind}, pad {pad}: {out}");
        }
    }
}

/// A list's `+=` and `extend` that run out of memory leave the list as it
/// was, as `CPython`'s do for a list source: its buffer grows once, to the
/// final length, before any item is written. They once kept the items
/// appended into the old buffer's room before the growth that failed.
#[test]
fn a_failed_list_extend_leaves_the_list_unchanged() {
    for (make, op) in [
        ("x = [0] * 1_000_000", "x += c"),
        ("class L(list): pass\nx = L([0] * 1_000_000)", "x += c"),
        ("x = [0] * 1_000_000", "x.extend(c)"),
        ("x = [0] * 1_000_000", "x += iter(c)"),
    ] {
        let src = format!(
            "{make}\nx.append(1)\nc = [2] * 2_000_000\nn0 = len(x)\n\
             try:\n    {op}\n    r = 'ok'\nexcept MemoryError:\n    r = 'ME'\n\
             print(r, len(x) - n0, x[-1])\n"
        );
        let (p, out) = run_with(Limits::new().with_memory(48 << 20), &src);
        assert!(matches!(p, Progress::Complete(_)), "{op}: {p:?}");
        assert_eq!(out, "ME 0 1\n", "{make}; {op}");
    }
}

/// Under a limit below 8 MiB, the next full segment (1 MiB) is more than
/// the 1/8 of the limit the pressure rule keeps free, so a heap of garbage
/// once failed to grow before any collection: a loop with almost nothing
/// live raised `MemoryError` at 1.5–3 MiB and passed at 1.25 and 4 MiB.
#[test]
fn garbage_never_fills_a_small_memory_limit() {
    for kib in [1536, 2048, 3072, 5120] {
        let limits = Limits::new().with_memory(kib << 10);
        let (p, out) = run_with(
            limits,
            "for i in range(20000):\n    x = [i] * 50\nprint('done')\n",
        );
        assert!(matches!(p, Progress::Complete(_)), "{kib} KiB: {p:?}");
        assert_eq!(out, "done\n", "{kib} KiB");
    }
}

/// A one-char str of a Latin-1 char is shared, as an ASCII one is and as
/// `CPython` shares them: the chars of a long text then cost a slot each. They
/// once cost a str each, about 74 B per char, so `tuple('é' * 8_000_000)`
/// failed at the default limit while `tuple('a' * 8_000_000)` fit.
#[test]
fn latin1_chars_are_shared_like_ascii_ones() {
    let src = "t = tuple('\\xe9' * 200_000)\nu = list('\\xff\\x80' * 100_000)\n\
               print(len(t), t[0] is t[-1], u[0] is chr(255), u[1] is '\\x80', \
               chr(0x100) is chr(0x100), t[0] == '\\xe9')\n";
    let (p, out) = run_with(Limits::new().with_memory(6 << 20), src);
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(out, "200000 True True True False True\n");
}

/// A cell that doubles a `repr` near the memory limit is bounded by fuel
/// and time: repr text costs fuel by its length (it once cost one unit per
/// item, so a cell with 20,000 fuel built tens of MB), and the buffer grows
/// geometrically up to the limit (it once grew by each write there, a copy
/// of the whole buffer per write).
#[test]
fn doubling_a_repr_near_the_memory_limit_is_bounded_by_fuel_and_time() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(16 << 20)));
    let code = engine
        .compile(
            "s = '\\x01'\nn = 0\ntry:\n    while True:\n        s = repr([s, ''])\n        n += 1\nexcept MemoryError:\n    pass\n(n, len(s))",
            "repr.py",
        )
        .expect("compiles");
    let started = std::time::Instant::now();
    let mut session = Session::new(&engine).expect("session");
    session.set_fuel(Some(20_000));
    let p = session.start(&code);
    assert!(
        matches!(
            p,
            Progress::Parked {
                reason: ParkReason::Fuel
            }
        ),
        "the doubling ran past its fuel: {p:?}"
    );
    session.set_fuel(None);
    let Ok(Progress::Complete(Some(c))) = session.resume(Resume::Continue) else {
        panic!("did not complete");
    };
    let (n, len) = c
        .repr
        .trim_matches(['(', ')'])
        .split_once(", ")
        .expect("pair");
    let (n, len): (u32, u64) = (n.parse().expect("n"), len.parse().expect("len"));
    assert!(
        n > 10 && len > 1 << 20,
        "stopped early: {n} steps, {len} bytes"
    );
    let elapsed = started.elapsed();
    assert!(elapsed.as_secs() < 20, "took {elapsed:?}");
}

#[path = "../../../fuzz/fuzz_targets/parse_input.rs"]
mod parse_input;

/// The `parse` and `compile` fuzz targets' seeds parse on their thread: the
/// address-sanitized build overflowed the old 1 MiB stack at 35 nesting
/// levels (this debug build does too), and 8 MiB on brackets behind a
/// comment ended by a lone `\r`, which the pre-scan did not count (a
/// release build needed 5 MiB). An error, not a crash, is the expected
/// outcome.
#[test]
fn parse_seeds_fit_the_fuzz_targets_stack() {
    for name in [
        "parse-asan-stack-overflow-d5d27626",
        "compile-stack-overflow-cr-comment-1a670fb6",
    ] {
        let path = format!(
            "{}/../../conformance/breadth/fuzz/artifacts/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let src = parse_input::source(&std::fs::read(path).expect("seed")).expect("source");
        let parsed = std::thread::Builder::new()
            .stack_size(parse_input::STACK)
            .spawn(move || mm_parse::parse(&src).map(|_| ()))
            .expect("spawn")
            .join()
            .expect("parses without a panic");
        assert!(parsed.is_err(), "{name}");
    }
}

#[path = "../../../fuzz/fuzz_targets/vm_bytecode_input.rs"]
mod vm_bytecode_input;

/// The `vm_bytecode` fuzz target's slow seeds (20,000 fuel, 64 MiB): a
/// cell that keeps escaping a growing `repr`, one `repr` of shared
/// containers that walked up to the memory limit (104M items, 18 s in a
/// debug build), caught `NameError`s that each computed their "Did you
/// mean" hint, and `in` scans of a long list that charged no fuel (33 s
/// without a sanitizer). The first is now bounded by fuel, the second parks
/// on fuel in the prelude twin after a walk of the fuel left, the third
/// computes the hint only when the exception is rendered, the fourth
/// charges one unit per item compared.
#[test]
fn vm_bytecode_slow_seeds_finish_promptly() {
    for (name, seconds) in [
        ("vm_bytecode-timeout-escape-17f52abd", 10),
        ("vm_bytecode-timeout-repr-13fd8ca2", 2),
        ("vm_bytecode-slow-namehint-7a9ffa8d-min", 2),
        ("vm_bytecode-slow-in-08c658ac-min", 2),
    ] {
        let path = format!(
            "{}/../../conformance/breadth/fuzz/artifacts/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let data = std::fs::read(path).expect("seed");
        let started = std::time::Instant::now();
        vm_bytecode_input::run(&data);
        let elapsed = started.elapsed();
        assert!(elapsed.as_secs() < seconds, "{name} took {elapsed:?}");
    }
}

/// The `vm_bytecode` crash from the 0.0.1 soak pre-run: verified bytecode
/// that ran `STA_DEREF` on a register holding a list replaced the list's
/// buffer with an int, and a later `del lst[1]` panicked. The deref ops
/// now raise `SystemError` on anything but a cell.
#[test]
fn vm_bytecode_deref_of_a_non_cell_does_not_panic() {
    let path = format!(
        "{}/../../conformance/breadth/fuzz/artifacts/vm_bytecode-crash-sta-deref-3edd3f97-min",
        env!("CARGO_MANIFEST_DIR")
    );
    vm_bytecode_input::run(&std::fs::read(path).expect("seed"));
}

#[test]
fn a_caught_name_error_costs_no_hint_until_it_is_rendered() {
    // Every NameError (and AttributeError) computed its "Did you mean"
    // over all globals and builtins when raised, uncharged, caught or not:
    // 200,000 caught ones took 8.35 s on mm, 0.06 s on CPython. CPython
    // computes it when the traceback is printed; so does mm now, and the
    // computation is charged as it runs (one unit per candidate name), so
    // fuel is the measure: a hint computed when raised costs the caught
    // errors 11.7M units for the names, 14.5M for the class's 201
    // attributes and 0.59M for the module's, against 51K for the cell [M].
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let listen = |session: &mut Session| {
        let sink = Arc::clone(&out);
        session.set_output(Box::new(move |s: &str| {
            sink.lock().expect("output lock").push_str(s);
        }));
    };
    let cell = |session: &mut Session, src: &str| {
        let code = engine.compile(src, "hint.py").expect("compiles");
        session.set_fuel(Some(u64::MAX / 2));
        let p = session.start(&code);
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        u64::MAX / 2 - session.fuel().expect("fuel")
    };
    let mut session = Session::new(&engine).expect("session");
    listen(&mut session);
    let fuel = cell(
        &mut session,
        "\
import traceback
class C:
    colour = 1
for k in range(200):
    setattr(C, f'colour{k}', k)
c = C()
for i in range(30000):
    try:
        pirnt
    except NameError as e:
        err = e
    try:
        c.color
    except AttributeError:
        pass
    try:
        traceback.sqr
    except AttributeError:
        pass
",
    );
    assert!(fuel < 200_000, "caught errors charged {fuel}");
    // The pending hint survives a snapshot, and reads as it always did.
    let image = session.snapshot().expect("snapshot");
    let mut session = engine.restore(&image).expect("restore");
    listen(&mut session);
    let fuel = cell(
        &mut session,
        "\
print(traceback.format_exception(err)[-1], end='')
for i in range(100):
    try:
        pirnt
    except NameError as e:
        traceback.format_exception(e)
",
    );
    assert_eq!(
        *out.lock().expect("output lock"),
        "NameError: name 'pirnt' is not defined. Did you mean: 'print'?\n"
    );
    // A hundred renders over some 200 candidates each.
    assert!(fuel > 20_000, "rendering charged {fuel}");
}

/// The repr seed's shape: shared containers whose `repr` describes far
/// more text than they hold.
const SHARED_GRAPH: &str = "a = [0]\nfor _ in range(20):\n    a = [a, a]\n";

#[test]
fn a_long_container_repr_inside_a_cell_parks_on_fuel() {
    // One native walk rendered the whole repr (7.3 MB here) between two
    // safepoints and charged it afterwards; with limited fuel it now hands
    // the rest to the prelude twin, which parks. The first park overdrew
    // 8.5M units; now only the twin's joins and its `'[' + body + ']'`
    // charge their copies (a unit per 64 bytes each, 115,000 units for the
    // 7.3 MB body) after them.
    let src = format!(
        "{SHARED_GRAPH}s = f'{{a}}'\nprint(len(s), len(repr([a])), len(str((a, 1))), len('%r' % (a,)))\n"
    );
    let module = mm_parse::parse(&src).expect("parses");
    let program = mm_compile::compile(&module, mm_compile::Options::default()).expect("compiles");
    let config = mm_engine::VmConfig {
        heap: mm_core::HeapConfig {
            memory_limit: 64 << 20,
            ..mm_core::HeapConfig::default()
        },
        recursion_limit: 200,
        fuel: Some(20_000),
        stdlib: mm_stdlib::MODULES,
    };
    let mut vm = mm_engine::Vm::new(config).expect("vm");
    let out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&out);
    vm.set_output(Box::new(move |_, s: &str| {
        sink.lock().expect("output lock").push_str(s);
    }));
    let entry = vm.load(&program, "graph.py", "").expect("loads");
    let mut exit = vm.run(entry);
    let mut parks = 0;
    while let mm_engine::Exit::Parked(mm_engine::ParkReason::Fuel) = exit {
        let (_, overdraft) = vm.fuel_account();
        assert!(overdraft < 300_000, "park {parks} overdrew {overdraft}");
        parks += 1;
        vm.set_fuel(Some(20_000));
        exit = vm.resume();
    }
    assert!(matches!(exit, mm_engine::Exit::Complete(_)), "{exit:?}");
    // The whole render's cost, in 20,000-unit slices: 4,384 [M].
    assert!((100..6_000).contains(&parks), "{parks} parks");
    assert_eq!(
        *out.lock().expect("output lock"),
        "7340028 7340030 7340033 7340028\n"
    );
}

#[test]
fn a_deep_container_repr_parked_on_fuel_costs_about_a_park_per_level() {
    // A repr the native walk cannot finish in the fuel left continues in
    // the prelude twin a nesting level at a time, parking about once per
    // level (3328 parks here). Each level also looked for itself among the
    // containers it is inside with a scan, some D²/2 units more for a
    // nesting D deep: 10522 parks here, where the unparked repr costs a
    // few units per level.
    let engine = Engine::new(EngineConfig::default());
    let code = engine
        .compile(
            "x = []\nfor _ in range(3000):\n    x = [x]\nlen(repr(x))",
            "deep.py",
        )
        .expect("compiles");
    let mut session = Session::new(&engine).expect("session");
    session.set_fuel(Some(500));
    let mut p = session.start(&code);
    let mut parks = 0;
    while let Progress::Parked { .. } = p {
        parks += 1;
        assert!(parks < 5000, "parked {parks} times");
        session.set_fuel(Some(500));
        p = session.resume(Resume::Continue).expect("resumes");
    }
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(c.repr, "6002");
}

#[test]
fn a_cut_completion_repr_stops_walking_past_what_the_cut_keeps() {
    // A completion `repr` cut head-and-tail rendered up to the memory
    // limit (here 64 MiB of a 117 MB text) to keep an end it never
    // reached; it now stops a bounded way past the limit and marks the
    // rest as omitted. The walk charges fuel by the byte (about 2.3 units
    // for the repr and str together [M]), so fuel measures how far it went:
    // a walk to the memory limit was a MemoryError, and one to 8 MiB costs
    // 19M units.
    let limits = Limits::new().with_memory(64 << 20);
    let engine = Engine::new(EngineConfig::new().with_limits(limits));
    let src = SHARED_GRAPH.replace("20", "24") + "a\n";
    let code = engine.compile(&src, "graph.py").expect("compiles");
    // What each cut walks: the limit for `Head`, max(8 x limit, 1 MiB) for
    // a container under `HeadTail`.
    for (cut, walk) in [
        (micro_monty::TextCut::Head, 1000),
        (micro_monty::TextCut::HeadTail, 1 << 20),
    ] {
        let mut session = Session::new(&engine).expect("session");
        session.set_text_limit(1000);
        session.set_text_cut(cut);
        session.set_fuel(Some(u64::MAX / 2));
        let p = session.start(&code);
        let fuel = u64::MAX / 2 - session.fuel().expect("fuel");
        let Progress::Complete(Some(c)) = p else {
            panic!("{p:?}")
        };
        assert!(c.repr_truncated && c.str_truncated, "{cut:?}");
        assert!(c.repr.len() <= 1000 && c.str.len() <= 1000, "{cut:?}");
        assert!(
            c.repr
                .starts_with("[[[[[[[[[[[[[[[[[[[[[[[[[0], [0]], [[0], [0]]"),
            "{}",
            c.repr
        );
        if cut == micro_monty::TextCut::HeadTail {
            assert!(c.repr.ends_with(" ... [more omitted]"), "{}", c.repr);
        }
        assert!(fuel < 3 * walk, "{cut:?} charged {fuel}");
    }
}

#[test]
fn memory_limit_raises_memory_error() {
    let limits = Limits::new().with_memory(8 << 20);
    let (p, out) = run_with(
        limits,
        "try:\n    x = [0] * 10_000_000\nexcept MemoryError:\n    print('caught')\nl = []\nwhile True:\n    l.append('x' * 1000)\n",
    );
    assert_eq!(out, "caught\n");
    assert_eq!(error_of(&p).type_name, "MemoryError");
    // A full heap raises the one preallocated MemoryError: each raise
    // starts it afresh, as CPython's free list does, so its traceback
    // does not grow by a raise's frames each time, nor keep a note or a
    // context (it grew 3, 6, 9 frames).
    let (p, out) = run_with(
        Limits::new().with_memory(8 << 20),
        "\
import traceback
def f(n):
    return f(n - 1) if n else {i: i for i in range(10**7)}
for i in range(3):
    try:
        try:
            f(1) if i != 1 else None
            raise KeyError if i == 1 else None
        except KeyError:
            f(1)
    except MemoryError as e:
        print(len(traceback.extract_tb(e.__traceback__)), type(e.__context__).__name__,
              hasattr(e, '__notes__'))
        e.add_note('n')
",
    );
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out,
        "3 NoneType False\n3 KeyError False\n3 NoneType False\n"
    );
    // An int result past the limit is refused before anything is built:
    // the shift's 2^40-bit result would otherwise ask the host allocator
    // for 128 GiB and abort the process.
    let (p, out) = run_with(
        Limits::new().with_memory(64 << 20),
        "\
def refused(f):
    try:
        f()
    except MemoryError:
        print('refused')
n = 1 << 40
a = 1 << 60_000_000
refused(lambda: 1 << n)
refused(lambda: 7 ** 1000 << n)
refused(lambda: a * a)
refused(lambda: 3 ** (n >> 10))
",
    );
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(out, "refused\n".repeat(4));
}

#[test]
fn no_object_is_bigger_than_64_mib_whatever_the_memory_limit() {
    // With 1 GiB to spend, 64 MiB of str data, list slots or bytearray is
    // the most one object holds: one more byte or slot is a MemoryError,
    // and growth one item at a time still reaches the cap.
    let (p, out) = run_with(
        Limits::new().with_memory(1 << 30),
        "\
def size(f):
    try:
        return len(f())
    except MemoryError:
        return 'MemoryError'
M = 64 << 20
print(size(lambda: 'a' * M), size(lambda: 'a' * (M + 1)), size(lambda: b'a' * (M + 1)))
print(size(lambda: [0] * (M // 8)), size(lambda: (0,) * (M // 8 + 1)))
x = [0] * 5_000_000
while len(x) < M // 8:
    x.append(0)
print(size(lambda: x), size(lambda: x.append(0) or x))
b = bytearray(M - 9)
for _ in range(9):
    b.append(1)
print(len(b), size(lambda: b.append(1) or b))
",
    );
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out,
        "67108864 MemoryError MemoryError\n8388608 MemoryError\n8388608 MemoryError\n67108864 MemoryError\n"
    );
}

#[test]
fn bytearray_growth_is_charged_to_the_memory_limit() {
    // A refused growth raises before the length changes; `clear()` drops
    // the block, so after a collection the same budget fits growth again.
    let (p, out) = run_with(
        Limits::new().with_memory(8 << 20),
        "
b = bytearray(3 << 20)
src = bytes(2 << 20)
def splice():
    b[0:0] = src
def repeat():
    global b
    b *= 3
for grow in (lambda: b.extend(src), splice, repeat):
    try:
        grow()
    except MemoryError:
        print('refused', len(b))
b.clear()
b += src
print(len(b))
while True:
    b += src
",
    );
    assert_eq!(
        out,
        "refused 3145728\nrefused 3145728\nrefused 3145728\n2097152\n"
    );
    assert_eq!(error_of(&p).type_name, "MemoryError");
}

#[test]
fn native_formatting_bounds_width_and_complex_precision() {
    let (p, out) = run_with(
        Limits::new().with_memory(8 << 20),
        "
for operation in [
    lambda: format(17, '2147483647d'),
    lambda: format(17, '02147483647_x'),
    lambda: format(1.25, '>2147483647f'),
    lambda: format('x', '02147483647'),
    lambda: format(1j, '>2147483647'),
    lambda: f'{True:2147483647d}',
    lambda: '{:2147483647}'.format('x'),
    lambda: format(1+2j, '.1073741824f'),
    lambda: format(1+2j, '.1073741824e'),
    lambda: format(1+2j, '#.1073741824g'),
]:
    try:
        operation()
    except MemoryError:
        pass
    else:
        raise AssertionError('oversized format succeeded')

# Large precision is legal when the result fits; Rust's float formatter has
# a smaller precision ceiling, so the native path must not pass it through.
text = format(1+2j, '.65536f')
assert text == '1.' + '0' * 65536 + '+2.' + '0' * 65536 + 'j'
assert format(1+2j, '.1073741824g') == '1+2j'
assert format(complex(float('inf'), float('inf')), '.1073741824f') == 'inf+infj'
assert format(complex(0, float('inf')), '#.1073741824') == 'infj'
assert format(-17, '🙂^9d') == '🙂🙂🙂-17🙂🙂🙂'
class Custom:
    def __format__(self, spec):
        return 'custom'
assert format(Custom(), '2147483647') == 'custom'
print('bounded and usable')
",
    );
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(out, "bounded and usable\n");
    // Width counts characters, but admission must include UTF-8 fill bytes.
    // Both the roughly 8 MiB native output and its arena copy must fit;
    // 16 MiB is insufficient once the rest of the session is counted too.
    for (memory, expected) in [
        (8 << 20, "bounded\n"),
        (16 << 20, "bounded\n"),
        (32 << 20, "2097152\n"),
    ] {
        let (p, out) = run_with(
            Limits::new().with_memory(memory),
            "try:\n    text = format('x', '🙂>2097152')\nexcept MemoryError:\n    print('bounded')\nelse:\n    assert text[-2:] == '🙂x'\n    print(len(text))",
        );
        assert!(matches!(p, Progress::Complete(_)), "{memory}: {p:?}");
        assert_eq!(out, expected, "{memory}");
    }
}

#[test]
fn starred_displays_calls_and_unpacking_copy_slot_to_slot() {
    // Results of 0.32, 0.22 and 0.22 of the limit beside inputs as large:
    // native copies of the items beside them did not fit [M].
    for (source, expected) in [
        (
            "M = 16 << 20\nl = [0] * (M * 32 // 1600)\nc = [1] * (M * 32 // 1600)\nx = [*l, *c]\nprint(len(x) == len(l) + len(c))",
            "True\n",
        ),
        (
            "M = 16 << 20\nl = [0] * (M * 22 // 1600)\nc = [1] * (M * 22 // 1600)\na, *b = [0, *l, *c]\nprint(len(b) == len(l) + len(c))",
            "True\n",
        ),
        (
            "M = 16 << 20\ndef f(*a):\n    return len(a)\nt = (0,) * (M * 22 // 1600)\nc = (1,) * (M * 22 // 1600)\nprint(f(*t, *c) == len(t) + len(c))",
            "True\n",
        ),
        (
            "s = 'é' * 1000\nprint(format(s, 's') is s, format(s, '') is s)",
            "True True\n",
        ),
    ] {
        let (p, out) = run_with(Limits::new().with_memory(16 << 20), source);
        assert!(matches!(p, Progress::Complete(_)), "{source}: {p:?}");
        assert_eq!(out, expected, "{source}");
    }
}

#[test]
fn string_builders_share_the_memory_budget_and_release_it_on_failure() {
    for expression in [
        "''.join([chunk] * 16)".into(),
        "('a' * 16).replace('a', chunk)".into(),
        "('a' * 16).translate({97: chunk})".into(),
        format!("f'{}'", "{chunk}".repeat(16)),
    ] {
        let source = format!(
            "chunk = 'é' * 131072\ntry:\n    result = {expression}\nexcept MemoryError:\n    print('bounded')\nelse:\n    print(len(result))\nprint('x'.join(['a', 'b']))"
        );
        for (memory, expected) in [(8 << 20, "bounded\naxb\n"), (16 << 20, "2097152\naxb\n")] {
            let (p, out) = run_with(Limits::new().with_memory(memory), &source);
            assert!(matches!(p, Progress::Complete(_)), "{expression}: {p:?}");
            assert_eq!(out, expected, "{expression}, memory={memory}");
        }
    }
}

#[test]
fn print_writes_what_precedes_a_failing_str() {
    // As CPython: the items and separators before the object whose
    // `__str__` raises are out; the rest and the ending are not.
    let (p, out) = run(
        "class X:\n    def __str__(self):\n        raise ValueError('boom')\ntry:\n    print('a', 1, X(), 'b', sep='-', end='!')\nexcept ValueError:\n    print('|caught')\ntry:\n    print(X(), 'b')\nexcept ValueError:\n    print('caught')",
    );
    assert_eq!(p, Progress::Complete(None));
    assert_eq!(out, "a-1-|caught\ncaught\n");
}

#[test]
fn print_writes_what_precedes_a_failing_int_conversion() {
    // CPython has written `a ` before `str(10 ** 5000)` raises.
    let (p, out) = run("try:\n    print('a', 10 ** 5000)\nexcept ValueError:\n    print('EXC')");
    assert_eq!(p, Progress::Complete(None));
    assert_eq!(out, "a EXC\n");
}

#[test]
fn repr_escapes_what_isprintable_rejects() {
    // Unassigned (U+0378, U+1AEC), variation selector supplement (Mn,
    // printable), plane-15 private use, and a plane-2 unassigned gap.
    let (p, _) = run(
        "repr(chr(0x378) + chr(0x1aec) + chr(0xe0100) + chr(0xf0000) + chr(0x2a6e0) + chr(0x2a6df))",
    );
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        c.str,
        "'\\u0378\\u1aec\u{e0100}\\U000f0000\\U0002a6e0\u{2a6df}'"
    );
}

#[test]
fn surrogates_are_refused_not_replaced() {
    // A `str` is UTF-8: CPython's lone surrogates are errors, never U+FFFD
    // or '?' (docs/user-guide.md, "Surrogates").
    let (p, _) = run(
        "r = []\nfor f in (lambda: chr(0xd800), lambda: '%c' % 0xdbff, lambda: format(0xdfff, 'c'), lambda: chr(0x110000)):\n    try:\n        f()\n    except ValueError as e:\n        r.append(str(e))\nr",
    );
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    let hold = "which micro-monty strings cannot hold: they are UTF-8";
    assert_eq!(
        c.str,
        format!(
            "['chr() arg is a surrogate (0xd800), {hold}', '%c arg is a surrogate (0xdbff), {hold}', \
             '%c arg is a surrogate (0xdfff), {hold}', 'chr() arg not in range(0x110000)']"
        )
    );
    // `json.loads` decodes a pair of escapes to its one char, as CPython
    // does, and refuses a lone one, also past its first window.
    let (p, _) = run(
        "import json\nr = [json.loads('\"\\\\ud83d\\\\ude00\"')]\nfor t in ('\"\\\\ud800\"', '[\"\\\\udc00\"]', '\"' + 'a' * 300_000 + '\\\\ud83d\"'):\n    try:\n        json.loads(t)\n    except ValueError as e:\n        r.append((type(e).__name__, e.pos))\nr",
    );
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        c.str,
        "['\u{1f600}', ('JSONDecodeError', 1), ('JSONDecodeError', 2), ('JSONDecodeError', 300001)]"
    );
}

#[test]
fn int_true_division_rounds_subnormal_quotients_once() {
    let (p, _) = run("(repr(6 / (int(1e300) << 30)), repr((2 ** 53 + 1) / 2 ** 1128))");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(c.str, "('5.587935447692873e-309', '5e-324')");
}

#[test]
fn functional_enum_with_only_a_type_is_empty() {
    let (p, out) = run(
        "from enum import Enum\nE = Enum('E', type=int)\nprint(E, len(E))\ntry:\n    Enum('F')\nexcept TypeError:\n    print('TypeError')",
    );
    assert_eq!(p, Progress::Complete(None));
    assert_eq!(out, "<enum 'E'> 0\nTypeError\n");
}

#[test]
fn printing_output_is_admitted_and_released_across_prelude_handoffs() {
    for custom in [false, true] {
        let item = if custom { "Text()" } else { "chunk" };
        let source = format!(
            "chunk = 'é' * 131072\ncalls = 0\nclass Text:\n    def __str__(self):\n        global calls\n        calls += 1\n        return chunk\nfor _ in range(2):\n    try:\n        print(*([chunk] * 8 + [{item}] + [chunk] * 23), sep='', end='!')\n    except MemoryError:\n        print('bounded')\n    else:\n        print('built')\nassert calls == {}\nprint('ok')\npass",
            if custom { 2 } else { 0 }
        );
        for memory in [8 << 20, 16 << 20] {
            let (p, out) = run_with(Limits::new().with_memory(memory), &source);
            assert_eq!(p, Progress::Complete(None), "{custom}, {memory}");
            let expected = if memory == 8 << 20 {
                "bounded\nbounded\nok\n".into()
            } else {
                // Eight MiB of UTF-8 text plus the ending fits; doubling the
                // buffer capacity would not. No partial output escapes a failure.
                let payload = "é".repeat(4_194_304);
                format!("{payload}!built\n{payload}!built\nok\n")
            };
            assert_eq!(out.len(), expected.len(), "{custom}, {memory}");
            assert!(out == expected, "wrong output: {custom}, {memory}");
        }
    }
}

#[test]
fn iterator_buffers_stay_budgeted_until_their_results_are_allocated() {
    for (expression, check, success) in [
        // A list iterator's items are gathered natively into a buffer (a
        // range's go into a list sized up front, with no buffer).
        ("list(iter([*range(300000)]))", "sum(result)", "44999850000"),
        (
            "''.join(iter(['é', '€', 'x'] * 100000))",
            "result[-3:]",
            "é€x",
        ),
    ] {
        let source = format!(
            "try:\n    result = {expression}\nexcept MemoryError:\n    print('bounded')\nelse:\n    print({check})\nprint(list(iter(range(3))))"
        );
        for memory in [8 << 20, 16 << 20] {
            let (p, out) = run_with(Limits::new().with_memory(memory), &source);
            assert!(matches!(p, Progress::Complete(_)), "{expression}: {p:?}");
            let expected = if memory == 8 << 20 {
                "bounded"
            } else {
                success
            };
            assert_eq!(
                out,
                format!("{expected}\n[0, 1, 2]\n"),
                "{expression}, {memory}"
            );
        }
    }
}

#[test]
fn native_sort_workspace_shares_the_memory_budget() {
    for reverse in ["False", "True"] {
        for memory in [6208 << 10, 16 << 20] {
            let engine =
                Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)));
            let mut session = Session::new(&engine).expect("session");
            let setup = engine
                .compile("data = [2, 0, 1] * 100000", "setup.py")
                .expect("compiles");
            assert_eq!(session.start(&setup), Progress::Complete(None));
            let code = engine
                .compile(
                    &format!(
                        "try:\n    data.sort(reverse={reverse})\nexcept MemoryError:\n    assert data[:3] == [2, 0, 1] and data[-3:] == [2, 0, 1]\n    bounded = True\nelse:\n    assert data[0] == (2 if {reverse} else 0)\n    assert data[99999] == data[0]\n    assert data[100000] == 1 and data[199999] == 1\n    assert data[200000] == (0 if {reverse} else 2)\n    assert data[-1] == data[200000]\n    bounded = False\nbounded"
                    ),
                    "sort.py",
                )
                .expect("compiles");
            let Progress::Complete(Some(c)) = session.start(&code) else {
                panic!("sort did not complete")
            };
            assert_eq!(
                c.repr,
                if memory == 6208 << 10 {
                    "True"
                } else {
                    "False"
                }
            );
            let recovery = engine
                .compile(
                    "data = []\nsmall = [3, -1, 2]\nsmall.sort()\nassert small == [-1, 2, 3]",
                    "recovery.py",
                )
                .expect("compiles");
            assert_eq!(session.start(&recovery), Progress::Complete(None));
            let image = session.snapshot().expect("snapshot");
            drop(session);
            let mut session = engine.restore(&image).expect("restore");
            assert_eq!(session.start(&recovery), Progress::Complete(None));
        }
    }
}

#[test]
fn json_output_is_admitted_before_building_and_arena_copying() {
    for (expression, length) in [
        ("json.dumps(data)", 4_194_306),
        ("json.dumps([data])", 4_194_308),
        ("json.dumps({'x': data})", 4_194_313),
        ("json.dumps(0j, default=lambda _: data)", 4_194_306),
    ] {
        let source = format!(
            "import json\ndata = '\"' * 2097152\ntry:\n    text = {expression}\nexcept MemoryError:\n    print('bounded')\nelse:\n    assert len(text) == {length}\n    print('built')\nprint(json.dumps([1, True, None]))"
        );
        // 64 KiB under what `dumps`' one growing block needs (7,446 KiB
        // [M]).
        for (memory, expected) in [
            (7382 << 10, "bounded\n[1, true, null]\n"),
            (20 << 20, "built\n[1, true, null]\n"),
        ] {
            let (p, out) = run_with(Limits::new().with_memory(memory), &source);
            assert!(matches!(p, Progress::Complete(_)), "{expression}: {p:?}");
            assert_eq!(out, expected, "{expression}, {memory}");
        }
    }
    // ASCII needs no sixfold reservation; astral characters need two UTF-16
    // escapes each when ensure_ascii is true, but only their UTF-8 bytes
    // otherwise (5,783 and 3,223 KiB [M]).
    for (source, memory, expected) in [
        (
            "import json\ntext = json.dumps('a' * 2097152)\nprint(len(text))",
            7232,
            "2097154\n",
        ),
        (
            "import json\ndata = '🙂' * 262144\ntry:\n    text = json.dumps(data)\nexcept MemoryError:\n    print('bounded')\nelse:\n    print('unbounded')\nprint(len(json.dumps(data, ensure_ascii=False)))",
            5719,
            "bounded\n262146\n",
        ),
    ] {
        let (p, out) = run_with(Limits::new().with_memory(memory << 10), source);
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        assert_eq!(out, expected);
    }
}

#[test]
fn sorted_json_entries_share_the_memory_budget() {
    // Sorted encoding refuses cleanly near the limit and recovers. Sorted
    // and plain dumps of this dict both fit from 4,224 KiB [M] since native
    // buffers grow geometrically up to the limit; the earlier 64 KiB window
    // where only the plain dump fit came from exact growth doubling once
    // more, for the closing brace, after the sorted entries were freed.
    for (memory, expected) in [(4208 << 10, "True"), (8 << 20, "False")] {
        let engine =
            Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)));
        let mut session = Session::new(&engine).expect("session");
        let setup = engine
            .compile(
                "import json\ndata = {i: None for i in range(65535, -1, -1)}",
                "setup.py",
            )
            .expect("compiles");
        assert_eq!(session.start(&setup), Progress::Complete(None));
        let code = engine
            .compile(
                "try:\n    text = json.dumps(data, sort_keys=True)\n    assert len(text) == 971930\n    assert text.startswith('{\"0\": null, \"1\": null, \"2\": null')\n    assert text.endswith('\"65535\": null}')\n    bounded = False\nexcept MemoryError:\n    bounded = True\nassert len(data) == 65536 and next(iter(data)) == 65535\nbounded",
                "encode.py",
            )
            .expect("compiles");
        let Progress::Complete(Some(c)) = session.start(&code) else {
            panic!("JSON encoding did not complete")
        };
        assert_eq!(c.repr, expected);
        let recovery = engine
            .compile(
                "text = None\ndata = None\nassert json.dumps({2: 7, -1: 3}, sort_keys=True) == '{\"-1\": 3, \"2\": 7}'",
                "recovery.py",
            )
            .expect("compiles");
        assert_eq!(session.start(&recovery), Progress::Complete(None));
        // Rebuilding the data needs nearly the whole limit (setup fails
        // below 4,048 KiB [M]): a charge the refusal leaked would fail it.
        assert_eq!(session.start(&setup), Progress::Complete(None));
        assert_eq!(session.start(&recovery), Progress::Complete(None));
        let image = session.snapshot().expect("snapshot");
        drop(session);
        let mut session = engine.restore(&image).expect("restore");
        assert_eq!(session.start(&setup), Progress::Complete(None));
        assert_eq!(session.start(&recovery), Progress::Complete(None));
    }
}

#[test]
fn json_decoder_buffers_share_the_memory_budget() {
    // The tight limits are 64 KiB under what each needs [M]. Whitespace is
    // read in place, without a buffer.
    for (tight, data, check) in [
        (
            7360 << 10,
            format!("[{}null]", "null,".repeat(262_143)),
            "assert len(result) == 262144 and result[0] is None and result[-1] is None",
        ),
        (
            7424 << 10,
            format!("{{{}\"k\":null}}", "\"k\":null,".repeat(131_071)),
            "assert result == {'k': None}",
        ),
        (
            3200 << 10,
            format!("\"{}\"", "\\n".repeat(1 << 20)),
            "assert len(result) == 1048576 and result[0] == '\\n' and result[-1] == '\\n'",
        ),
        (
            3200 << 10,
            format!("{{\"{}\":7}}", "\\n".repeat(1 << 20)),
            "assert len(result) == 1 and len(next(iter(result))) == 1048576 and next(iter(result.values())) == 7",
        ),
    ] {
        for (memory, expected) in [(tight, "True"), (16 << 20, "False")] {
            let engine =
                Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)));
            let mut session = Session::new(&engine).expect("session");
            session
                .define(&HostModule::untyped("", &["source"]))
                .expect("defined");
            let setup = engine
                .compile("import json\ndata = source()", "setup.py")
                .expect("compiles");
            assert!(matches!(session.start(&setup), Progress::HostCall { .. }));
            assert_eq!(
                session
                    .resume(Resume::Value(micro_monty::HostValue::Str(data.clone())))
                    .expect("host reply"),
                Progress::Complete(None)
            );
            let code = engine
                .compile(
                    &format!("try:\n    result = json.loads(data)\n    bounded = False\nexcept MemoryError:\n    bounded = True\nif not bounded:\n    {check}\nassert json.loads('[1, true, null]') == [1, True, None]\nbounded"),
                    "decode.py",
                )
                .expect("compiles");
            let Progress::Complete(Some(c)) = session.start(&code) else {
                panic!("JSON decoding did not complete")
            };
            assert_eq!(c.repr, expected, "{check}, {memory}");
        }
    }
}

#[test]
fn json_traversal_stacks_share_the_memory_budget() {
    for (tight, setup, operation, check) in [
        // Decoding keeps one value stack for all open containers, so the
        // nested inputs fit in about 4.5 MiB [M]; the tight limits are 64 KiB
        // under what each needs.
        (
            4544 << 10,
            "data = '[' * 50000 + '0' + ']' * 50000",
            "json.loads(data)",
            "for _ in range(50000):\n        assert len(result) == 1\n        result = result[0]\n    assert result == 0",
        ),
        (
            4288 << 10,
            "data = '{\"k\":' * 50000 + '0' + '}' * 50000",
            "json.loads(data)",
            "for _ in range(50000):\n        assert len(result) == 1\n        result = result['k']\n    assert result == 0",
        ),
        (
            4928 << 10,
            "data = 0\nfor _ in range(50000):\n    data = [data]",
            "json.dumps(data, check_circular=False)",
            "assert result == '[' * 50000 + '0' + ']' * 50000",
        ),
        (
            6336 << 10,
            "data = 0\nfor _ in range(50000):\n    data = {'k': data}",
            "json.dumps(data)",
            "assert result == '{\"k\": ' * 50000 + '0' + '}' * 50000",
        ),
        (
            6336 << 10,
            "data = 0\nfor _ in range(50000):\n    data = [data]\nassert json.dumps(data, check_circular=False) == '[' * 50000 + '0' + ']' * 50000",
            "json.dumps(data)",
            "assert result == '[' * 50000 + '0' + ']' * 50000",
        ),
    ] {
        for (memory, expected) in [(tight, "True"), (16 << 20, "False")] {
            let engine =
                Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)));
            let mut session = Session::new(&engine).expect("session");
            let setup = engine
                .compile(&format!("import json\n{setup}"), "setup.py")
                .expect("compiles");
            assert_eq!(session.start(&setup), Progress::Complete(None));
            let code = engine
                .compile(
                    &format!("try:\n    result = {operation}\n    bounded = False\nexcept MemoryError:\n    bounded = True\nif not bounded:\n    {check}\nassert json.loads('[7]') == [7]\nassert json.dumps({{'a': [3]}}) == '{{\"a\": [3]}}'\nbounded"),
                    "json.py",
                )
                .expect("compiles");
            let p = session.start(&code);
            let Progress::Complete(Some(c)) = p else {
                panic!("JSON traversal did not complete: {p:?}")
            };
            assert_eq!(c.repr, expected, "{operation}, {memory}");
            let image = session.snapshot().expect("snapshot");
            drop(session);
            let mut session = engine.restore(&image).expect("restore");
            let recovery = engine
                .compile(
                    "data = None\nresult = None\nassert json.loads('{\"x\": [2]}') == {'x': [2]}\nassert json.dumps([8, 4]) == '[8, 4]'",
                    "recovery.py",
                )
                .expect("compiles");
            assert_eq!(session.start(&recovery), Progress::Complete(None));
        }
    }
}

#[test]
fn rendered_text_stays_budgeted_through_conversion() {
    for (operation, length) in [
        ("repr([data] * 4)", 786_448),
        ("ascii(data * 2)", 1_048_578),
        ("str(OSError(7, data, data, None, data))", 524_308),
    ] {
        for (memory, expected) in [
            (2112 << 10, "bounded".into()),
            (8 << 20, length.to_string()),
        ] {
            let source = format!(
                "data = '€\\n' * 65536\ntry:\n    text = {operation}\nexcept MemoryError:\n    print('bounded')\nelse:\n    print(len(text))\nassert repr([7, 3]) == '[7, 3]'"
            );
            let (progress, output) = run_with(Limits::new().with_memory(memory), &source);
            assert_eq!(progress, Progress::Complete(None), "{operation}, {memory}");
            assert_eq!(output, format!("{expected}\n"), "{operation}, {memory}");
        }
    }
}

#[test]
fn repr_traversal_shares_the_memory_budget() {
    for (tight, wrap, check) in [
        (6016 << 10, "[data]", "'[' * 30000 + '7' + ']' * 30000"),
        (4 << 20, "(data,)", "'(' * 30000 + '7' + ',)' * 30000"),
        (
            4 << 20,
            "{'k': data}",
            "\"{'k': \" * 30000 + '7' + '}' * 30000",
        ),
    ] {
        for (memory, expected) in [(tight, "True"), (8 << 20, "False")] {
            let engine =
                Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)));
            let mut session = Session::new(&engine).expect("session");
            let setup = engine
                .compile(
                    &format!("data = 7\nfor _ in range(30000):\n    data = {wrap}"),
                    "setup.py",
                )
                .expect("compiles");
            assert_eq!(session.start(&setup), Progress::Complete(None));
            let code = engine
                .compile(
                    &format!("try:\n    text = repr(data)\n    bounded = False\nexcept MemoryError:\n    bounded = True\nif not bounded:\n    assert text == {check}\nassert repr([7, 3]) == '[7, 3]'\nbounded"),
                    "repr.py",
                )
                .expect("compiles");
            // Linear in the nesting: about 2M units for 30,000 dicts. Past
            // the native walk's slice the prelude went on level by level,
            // trying a whole slice again at each and building each level's
            // text, 555M units and 30 s in release (9589c84) [M].
            session.set_fuel(Some(20_000_000));
            let p = session.start(&code);
            let Progress::Complete(Some(c)) = p else {
                panic!("repr traversal did not complete within 20M fuel: {p:?}")
            };
            assert_eq!(c.repr, expected, "{wrap}, {memory}");
            let image = session.snapshot().expect("snapshot");
            drop(session);
            let mut session = engine.restore(&image).expect("restore");
            let recovery = engine
                .compile(
                    "data = None\ntext = None\nassert repr({'a': [8, 4]}) == \"{'a': [8, 4]}\"",
                    "recovery.py",
                )
                .expect("compiles");
            assert_eq!(session.start(&recovery), Progress::Complete(None));
        }
    }
}

#[test]
fn percent_buffers_share_the_memory_budget() {
    for (format, length) in [
        (format!("%{}s", "0".repeat(4 << 20)), 1),
        (String::from("%4194304s"), 4 << 20),
    ] {
        for bytes in [false, true] {
            for (memory, expected) in [(8 << 20, "True"), (12 << 20, "False")] {
                let engine =
                    Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)));
                let mut session = Session::new(&engine).expect("session");
                session
                    .define(&HostModule::untyped("", &["source"]))
                    .expect("defined");
                let setup = engine
                    .compile("data = source()", "setup.py")
                    .expect("compiles");
                assert!(matches!(session.start(&setup), Progress::HostCall { .. }));
                let data = if bytes {
                    micro_monty::HostValue::Bytes(format.as_bytes().to_vec())
                } else {
                    micro_monty::HostValue::Str(format.clone())
                };
                assert_eq!(
                    session.resume(Resume::Value(data)).expect("host reply"),
                    Progress::Complete(None)
                );
                let prefix = if bytes { "b" } else { "" };
                let code = engine
                    .compile(
                        &format!(
                            "try:\n    text = data % {prefix}'x'\n    bounded = False\nexcept MemoryError:\n    bounded = True\nif not bounded:\n    assert len(text) == {length}\n    assert text.endswith({prefix}'x')\n    assert text.count({prefix}' ') == {length} - 1\nassert {prefix}'%03d:%s' % (7, {prefix}'ok') == {prefix}'007:ok'\nbounded"
                        ),
                        "percent.py",
                    )
                    .expect("compiles");
                let p = session.start(&code);
                let Progress::Complete(Some(c)) = p else {
                    panic!("percent formatting did not complete: {p:?}")
                };
                // A long result is built in its arena block, piece by
                // piece, with no native buffer: it fits.
                let expected = if length > 1 { "False" } else { expected };
                assert_eq!(
                    c.repr, expected,
                    "length={length}, bytes={bytes}, memory={memory}"
                );
                let image = session.snapshot().expect("snapshot");
                drop(session);
                let mut session = engine.restore(&image).expect("restore");
                let recovery = engine
                    .compile(
                        "data = None\ntext = None\nassert '%-5s:%04d' % ('ok', -7) == 'ok   :-007'\nassert b'%5s:%04d' % (b'ok', -7) == b'   ok:-007'",
                        "recovery.py",
                    )
                    .expect("compiles");
                assert_eq!(session.start(&recovery), Progress::Complete(None));
            }
        }
    }
}

#[test]
fn byte_builders_keep_their_capacity_charged_during_arena_allocation() {
    // Byte results are built in their final arena block (bytes.rs), so the
    // peak is the result itself: 4 MiB cannot hold it on top of the session.
    for expression in [
        "bytes(4194304)",
        "chunk * 16",
        "b''.join([chunk] * 16)",
        "(b'a' * 16).replace(b'a', chunk)",
        "b'x'.ljust(4194304)",
        "b'x'.rjust(4194304)",
        "b'x'.center(4194304)",
        "b'-1'.zfill(4194304)",
        "(b'\\t' * 16).expandtabs(262144)",
    ] {
        let source = format!(
            "chunk = b'\\xff' * 262144\ntry:\n    result = {expression}\nexcept MemoryError:\n    print('bounded')\nelse:\n    print(len(result))\nprint(b'x'.join([b'a', b'b']))"
        );
        for (memory, expected) in [
            (4 << 20, "bounded\nb'axb'\n"),
            (16 << 20, "4194304\nb'axb'\n"),
        ] {
            let (p, out) = run_with(Limits::new().with_memory(memory), &source);
            assert!(matches!(p, Progress::Complete(_)), "{expression}: {p:?}");
            assert_eq!(out, expected, "{expression}, memory={memory}");
        }
    }
}

#[test]
fn replace_budgets_actual_matches_and_utf8_bytes() {
    let (p, _) = run_with(
        Limits::new().with_memory(8 << 20),
        "s = 'é' * 16384\nreplacement = '€' * 4096\nassert s.replace('absent', replacement) == s\nassert s.replace('', replacement, 0) == s\nassert s.replace('', replacement, 1) == replacement + s\nassert s.replace('é', '€', -1) == '€' * 16384\nassert 'aaaa'.replace('aa', '€', 1) == '€aa'",
    );
    assert_eq!(p, Progress::Complete(None));
}

#[test]
fn byte_case_transforms_share_the_builder_budget() {
    for (method, prefix) in [
        ("upper", "AB CX!"),
        ("lower", "ab cx!"),
        ("swapcase", "Ab Cx!"),
        ("capitalize", "Ab cx!"),
        ("title", "Ab Cx!"),
    ] {
        let source = format!(
            "chunk = b'aB cX!' * 393216\ntry:\n    result = chunk.{method}()\nexcept MemoryError:\n    print('bounded')\nelse:\n    assert result[:6] == b'{prefix}'\n    print(len(result))\nassert b'aBc'.upper() == b'ABC'"
        );
        // Built in place: source and result (4.5 MiB) exceed 4 MiB; a
        // second, `Vec`-sized copy would not fit at 6208 KiB either.
        for (memory, expected) in [
            (4 << 20, "bounded\n"),
            (6208 << 10, "2359296\n"),
            (16 << 20, "2359296\n"),
        ] {
            let (p, out) = run_with(Limits::new().with_memory(memory), &source);
            assert!(matches!(p, Progress::Complete(_)), "{method}: {p:?}");
            assert_eq!(out, expected, "{method}, memory={memory}");
        }
    }
}

#[test]
fn unchanged_byte_results_succeed_at_memory_pressure() {
    let (p, _) = run_with(
        Limits::new().with_memory(8 << 20),
        "chunk = b'x' * 2097152\nkeep = []\ntry:\n    while True:\n        keep.append(bytes(262144))\nexcept MemoryError:\n    pass\nassert chunk.ljust(0) == chunk\nassert chunk.rjust(-1) == chunk\nassert chunk.center(2097152) == chunk\nassert chunk.zfill(2097151) == chunk\nassert chunk[:] == chunk\nassert chunk.strip() == chunk\nassert chunk.lstrip(b'z') == chunk\nassert chunk.rstrip(b'') == chunk\nassert chunk.removeprefix(b'z') == chunk\nassert chunk.removesuffix(b'') == chunk\nassert chunk.partition(b'z')[0] == chunk\nassert chunk.rpartition(b'z')[2] == chunk\nassert chunk.split(b'z') == [chunk]\nassert chunk.rsplit(None, 0) == [chunk]\nassert chunk.splitlines() == [chunk]",
    );
    assert_eq!(p, Progress::Complete(None));
}

#[test]
fn byte_parts_share_the_budget() {
    for expression in [
        "chunk[1:]",
        "chunk[::-1]",
        "chunk.rstrip(b'y')",
        "chunk.removeprefix(b'x')",
        "chunk.removesuffix(b'y')",
        "chunk.partition(b'x')[2]",
        "chunk.rpartition(b'y')[0]",
        "chunk.split(b'x', 1)[1]",
        "chunk.rsplit(b'y', 1)[0]",
    ] {
        let source = format!(
            "chunk = b'xy' * 1179648\ntry:\n    result = {expression}\nexcept MemoryError:\n    print('bounded')\nelse:\n    print(len(result))\nassert b'abc'.split(b'b') == [b'a', b'c']"
        );
        let length = if expression == "chunk[::-1]" {
            "2359296"
        } else {
            "2359295"
        };
        // Built in place: source and result (4.5 MiB) exceed 4 MiB.
        for (memory, expected) in [(4 << 20, "bounded"), (16 << 20, length)] {
            let (p, out) = run_with(Limits::new().with_memory(memory), &source);
            assert_eq!(p, Progress::Complete(None), "{expression}: {p:?}");
            assert_eq!(
                out,
                format!("{expected}\n"),
                "{expression}, memory={memory}"
            );
        }
    }
    for method in ["split", "rsplit", "splitlines"] {
        let source = format!(
            "chunk = b'x\\n' * 131072\ntry:\n    result = chunk.{method}()\nexcept MemoryError:\n    print('bounded')\nelse:\n    print(len(result))\nassert b'a b'.split() == [b'a', b'b']"
        );
        // A split job counts the parts first and sizes the list once, so
        // only the parts and the list itself (8.5 MiB) count.
        for (memory, expected) in [(8 << 20, "bounded"), (16 << 20, "131072")] {
            let (p, out) = run_with(Limits::new().with_memory(memory), &source);
            assert_eq!(p, Progress::Complete(None), "{method}: {p:?}");
            assert_eq!(out, format!("{expected}\n"), "{method}, memory={memory}");
        }
    }
}

#[test]
fn byte_conversion_outputs_share_the_budget() {
    // `tight` (MiB) fits the source but not the result on top of it: the
    // conversions are byte jobs that write into the result's final block.
    for (setup, expression, length, tight, roomy) in [
        ("chunk = b'xy' * 786432", "chunk.hex()", "3145728", 4, 16),
        (
            "chunk = b'xy' * 786432",
            "chunk.hex(':', -3)",
            "3670015",
            4,
            16,
        ),
        (
            "text = 'ab' * 1048576",
            "bytes.fromhex(text)",
            "1048576",
            3,
            8,
        ),
        ("items = [1] * 1048576", "bytes(items)", "1048576", 9, 12),
        ("s = 'xy' * 1179648", "s.encode()", "2359296", 4, 16),
        ("s = 'xy' * 1179648", "s.encode('ascii')", "2359296", 4, 16),
        (
            "s = 'xy' * 1179648",
            "bytes(s, 'latin-1')",
            "2359296",
            4,
            16,
        ),
        (
            "s = 'λ' * 524288",
            "s.encode('ascii', 'backslashreplace')",
            "3145728",
            4,
            16,
        ),
        (
            "s = '𐀀' * 393216",
            "s.encode('ascii', 'xmlcharrefreplace')",
            "3145728",
            4,
            16,
        ),
        ("s = b'xy' * 1179648", "s.decode()", "2359296", 4, 16),
        ("s = b'xy' * 1179648", "str(s, 'ascii')", "2359296", 4, 16),
        (
            "s = b'\\xff' * 1572864",
            "s.decode('latin-1')",
            "1572864",
            4,
            16,
        ),
        (
            "s = b'\\xffx' * 786432",
            "s.decode('utf-8', 'replace')",
            "1572864",
            4,
            16,
        ),
    ] {
        let source = format!(
            "{setup}\ntry:\n    result = {expression}\nexcept MemoryError:\n    print('bounded')\nelse:\n    print(len(result))\nassert b'\\xffa'.hex() == 'ff61'"
        );
        for (memory, expected) in [(tight << 20, "bounded"), (roomy << 20, length)] {
            let (p, out) = run_with(Limits::new().with_memory(memory), &source);
            assert_eq!(p, Progress::Complete(None));
            assert_eq!(
                out,
                format!("{expected}\n"),
                "{expression}, memory={memory}"
            );
        }
    }
    let (p, out) = run_with(
        Limits::new().with_memory(8 << 20),
        "text = ('ab' + ' ' * 30) * 131072\nprint(len(bytes.fromhex(text)))",
    );
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(out, "131072\n");
    for source in [
        "chunk = 'λ' * 1048576\nprint(len(chunk.encode('ascii', 'ignore')))",
        "chunk = b'\\xff' * 2097152\nprint(len(chunk.decode('ascii', 'ignore')))",
    ] {
        let (p, out) = run_with(Limits::new().with_memory(8 << 20), source);
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        assert_eq!(out, "0\n");
    }
}

#[test]
fn codec_names_are_borrowed_and_lookup_errors_are_bounded() {
    for expression in [
        "'x'.encode(name)",
        "bytes('x', name)",
        "b'x'.decode(name)",
        "str(b'x', name)",
        "'λ'.encode('ascii', name)",
        "b'\\xff'.decode('ascii', name)",
    ] {
        let source = format!(
            "name = 'z' * 3145728\ntry:\n    {expression}\nexcept MemoryError:\n    print('bounded')\nexcept LookupError:\n    print('lookup')\nassert 'x'.encode('ascii', name) == b'x'\nassert b'x'.decode('ascii', name) == 'x'"
        );
        for (memory, expected) in [(8256 << 10, "bounded\n"), (16 << 20, "lookup\n")] {
            let (p, out) = run_with(Limits::new().with_memory(memory), &source);
            assert_eq!(p, Progress::Complete(None), "{expression}");
            assert_eq!(out, expected, "{expression}, memory={memory}");
        }
    }
}

#[test]
fn execution_stacks_share_the_memory_budget() {
    let mut locals = String::new();
    for i in 0..200 {
        writeln!(locals, "    a{i} = {i}").expect("string");
    }
    let source = format!(
        "def f(n):\n{locals}    if n == 700:\n        return n\n    return f(n + 1)\ntry:\n    print(f(0))\nexcept MemoryError:\n    print('bounded')\n"
    );
    for (memory, expected) in [(1088 << 10, "bounded\n"), (8 << 20, "700\n")] {
        let (p, out) = run_with(Limits::new().with_memory(memory), &source);
        assert_eq!(out, expected);
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    }
}

#[test]
fn scheduler_tables_share_the_memory_budget() {
    let source = "import asyncio\ntry:\n    result = await asyncio.gather(*[asyncio.sleep(1, i) for i in range(4500)])\n    print(len(result), result[-1])\nexcept MemoryError:\n    print('bounded')\n";
    // 4,500 pending tasks fit in about 2.8 MiB (4 MiB before natives
    // charged their results: collections now come sooner).
    for (memory, expected) in [(2 << 20, "bounded\n"), (8 << 20, "4500 4499\n")] {
        let (p, out) = run_with(Limits::new().with_memory(memory), source);
        assert_eq!(out, expected);
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    }
}

#[test]
fn instance_shapes_share_the_memory_budget() {
    let source = "class Row: pass\nkeep = []\ntry:\n    for i in range(3000):\n        row = Row()\n        for j in range(5):\n            setattr(row, 'a' + str(j) + '_' + str((i >> (j * 3)) & 7), j)\n        keep.append(row)\n    print(len(keep), keep[-1].a4_0)\nexcept MemoryError:\n    print('bounded')\n";
    for (memory, expected) in [(3392 << 10, "bounded\n"), (8 << 20, "3000 4\n")] {
        let (p, out) = run_with(Limits::new().with_memory(memory), source);
        assert_eq!(out, expected);
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    }
}

#[test]
fn failed_set_growth_leaves_the_set_usable() {
    let (p, out) = run_with(
        Limits::new().with_memory(8 << 20),
        "\
s = {-1, -2}
keep = []
try:
    while True:
        keep.append([0] * 1024)
except MemoryError:
    pass
failed = 0
for i in range(2048):
    before = len(s)
    try:
        s.add(i)
    except MemoryError:
        failed += 1
        assert len(s) == before
        assert i not in s
assert failed > 0
assert -1 in s and -2 in s
s.remove(-1)
assert -1 not in s
s.add(-1)
assert -1 in s
print('usable after failed growth')
",
    );
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(out, "usable after failed growth\n");
}

#[test]
fn fuel_parks_and_resumes() {
    let limits = Limits::new().with_fuel(Some(10_000));
    let engine = Engine::new(EngineConfig::new().with_limits(limits));
    let code = engine
        .compile("t = 0\nfor i in range(100000):\n    t += i\nt", "fuel.py")
        .expect("compiles");
    let mut session = Session::new(&engine).expect("session");
    let mut progress = session.start(&code);
    let mut parks = 0;
    while let Progress::Parked { reason } = progress {
        assert_eq!(reason, ParkReason::Fuel);
        parks += 1;
        session.set_fuel(Some(10_000));
        progress = session.resume(Resume::Continue).expect("resumes");
    }
    assert!(parks > 1, "parked {parks} times");
    let Progress::Complete(Some(c)) = progress else {
        panic!("{progress:?}")
    };
    assert_eq!(c.repr, "4999950000");
}

#[test]
fn native_sort_charges_fuel_and_survives_restore() {
    for operation in ["data.sort()", "data = sorted(data)"] {
        let engine = Engine::new(EngineConfig::default());
        let mut session = Session::new(&engine).expect("session");
        let setup = engine
            .compile("data = list(range(4095, -1, -1))", "setup.py")
            .expect("compiles");
        assert_eq!(session.start(&setup), Progress::Complete(None));
        let code = engine
            .compile(
                &format!("{operation}\n[data[0], data[-1], len(data)]"),
                "sort.py",
            )
            .expect("compiles");
        session.set_fuel(Some(100));
        assert_eq!(
            session.start(&code),
            Progress::Parked {
                reason: ParkReason::Fuel
            },
            "{operation}"
        );
        let snapshot = session.snapshot().expect("snapshot");
        session = engine.restore(&snapshot).expect("restore");
        session.set_fuel(None);
        let progress = session.resume(Resume::Continue).expect("resume");
        let Progress::Complete(Some(c)) = progress else {
            panic!("{progress:?}")
        };
        assert_eq!(c.repr, "[0, 4095, 4096]");
    }
}

#[test]
fn shared_container_text_charges_fuel_and_survives_restore() {
    let engine = Engine::new(EngineConfig::default());
    let mut session = Session::new(&engine).expect("session");
    let setup = engine
        .compile("x = [0]\nfor _ in range(8):\n    x = [x, x, x]", "graph.py")
        .expect("compiles");
    assert_eq!(session.start(&setup), Progress::Complete(None));
    for expression in [
        "repr(x)",
        "str(x)",
        "str(object=x)",
        "x.__repr__()",
        "repr(*(x,))",
    ] {
        let code = engine
            .compile(&format!("s = {expression}\nlen(s)"), "text.py")
            .expect("compiles");
        session.set_fuel(Some(100));
        assert_eq!(
            session.start(&code),
            Progress::Parked {
                reason: ParkReason::Fuel
            },
            "{expression}"
        );
        let snapshot = session.snapshot().expect("snapshot");
        session = engine.restore(&snapshot).expect("restore");
        session.set_fuel(None);
        let progress = session.resume(Resume::Continue).expect("resume");
        let Progress::Complete(Some(c)) = progress else {
            panic!("{progress:?}")
        };
        // Starting at len('[0]') = 3, each level triples the length and
        // adds two brackets and two ', ' separators: 6 * 3**8 - 3.
        assert_eq!(c.repr, "39363");
    }
}

#[test]
fn garbage_is_collected_during_long_runs() {
    let limits = Limits::new().with_memory(16 << 20);
    // Allocates far more than the limit in total, little of it live.
    let (p, out) = run_with(
        limits,
        "def mk(i):\n    return [str(i)] * 10\nkeep = []\nfor i in range(300000):\n    x = mk(i)\n    if i % 1000 == 0:\n        keep.append(x)\nprint(len(keep), keep[-1][0])\n",
    );
    assert_eq!(out, "300 299000\n");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
}

#[test]
fn match_pattern_errors_are_cpythons_compile_errors() {
    let engine = Engine::new(EngineConfig::default());
    for (case, msg) in [
        (
            "case [x, ('a' | 'b') as y, ([x] | (0, x))]:",
            "multiple assignments to name 'x' in pattern",
        ),
        (
            "case x:\n        pass\n    case 1:",
            "name capture 'x' makes remaining patterns unreachable",
        ),
        (
            "case {1: a, 1.0: b}:",
            "mapping pattern checks duplicate key (1.0)",
        ),
        (
            "case {b'a': 1, b'a': 2}:",
            "mapping pattern checks duplicate key (b'a')",
        ),
        (
            "case {1e999: 1, 1e999: 2}:",
            "mapping pattern checks duplicate key (inf)",
        ),
        (
            "case [a] | [b]:",
            "alternative patterns bind different names",
        ),
        (
            "case C(x=1, x=2):",
            "attribute name repeated in class pattern: x",
        ),
    ] {
        let src = format!("match v:\n    {case}\n        pass\n");
        let err = engine.compile(&src, "m.py").expect_err(&src);
        assert_eq!(err.type_name, "SyntaxError");
        assert_eq!(err.message, msg);
    }
}

#[test]
fn break_continue_and_return_in_except_star_are_cpythons_syntax_errors() {
    let engine = Engine::new(EngineConfig::default());
    let msg = "'break', 'continue' and 'return' cannot appear in an except* block";
    for (body, line, column) in [
        (
            "for x in y:\n    try:\n        pass\n    except* E:\n        continue\n",
            5,
            8,
        ),
        (
            "while 1:\n    try:\n        pass\n    except* E:\n        if x:\n            break\n",
            6,
            12,
        ),
        // At a literal value, else at the statement (`-1` is folded).
        (
            "def f():\n    try:\n        pass\n    except* E:\n        return  'v'\n",
            5,
            16,
        ),
        (
            "def f():\n    try:\n        pass\n    except* E:\n        return  -1\n",
            5,
            8,
        ),
    ] {
        let err = engine.compile(body, "s.py").expect_err(body);
        assert_eq!(err.type_name, "SyntaxError");
        assert_eq!(err.message, msg);
        assert_eq!((err.line, err.column), (line, column), "{body}");
    }
    // A loop or function inside the block is its own.
    let ok = "def f():\n    try:\n        pass\n    except* E:\n        for i in x:\n            break\n        def g():\n            return 1\n";
    engine.compile(ok, "s.py").expect("compiles");
}

#[test]
fn unsupported_syntax_is_a_compile_error() {
    let engine = Engine::new(EngineConfig::default());
    for (src, what) in [
        ("class A:\n    __slots__ = ()\n", "__slots__"),
        (
            "class A:\n    def __del__(self):\n        pass\n",
            "__del__",
        ),
        (
            "class A:\n    def __getattribute__(self, name):\n        return 1\n",
            "__getattribute__",
        ),
    ] {
        let err = engine.compile(src, "u.py").expect_err(src);
        assert_eq!(err.type_name, "SyntaxError");
        assert!(
            err.message.starts_with("micro-monty does not support"),
            "{}",
            err.message
        );
        assert!(err.message.contains(what), "{}", err.message);
    }
}

#[test]
fn code_past_the_compilers_limits_is_a_compile_error_and_just_under_runs() {
    // Each limit's two sides: one past it raises the SyntaxError naming
    // it, one at it compiles and runs. (The u8/u16 operand checks behind
    // these, such as "f-strings this large", are reached only after one
    // of these limits.)
    let locals = |n: usize| {
        let body = (0..n).fold(String::new(), |mut s, i| {
            let _ = writeln!(s, "    v{i} = {i}");
            s
        });
        format!("def f():\n{body}    return v0\nf()\n")
    };
    // Unpacking holds every value in a register before the first store.
    let unpack = |n: usize| {
        let targets: Vec<String> = (0..n).map(|i| format!("a{i}")).collect();
        format!("{} = range({n})\n", targets.join(", "))
    };
    let big_source = format!("x = '{}'\n", "a".repeat(256 * 1024));
    let engine = Engine::new(EngineConfig::default());
    for (past, at, what) in [
        (
            locals(251),
            locals(250),
            "functions with more than 250 local variables",
        ),
        (
            unpack(256),
            unpack(255),
            "expressions needing more than 255 registers",
        ),
        (
            big_source,
            "x = 1\n".to_owned(),
            "source files larger than 256 KiB",
        ),
    ] {
        let Err(err) = engine.compile(&past, "limit.py") else {
            panic!("{what}: compiled");
        };
        assert_eq!(
            (err.type_name.as_str(), err.message.as_str()),
            (
                "SyntaxError",
                format!("micro-monty does not support {what}").as_str()
            ),
        );
        let (p, _) = run(&at);
        assert!(matches!(p, Progress::Complete(_)), "{what}: {p:?}");
    }
}

#[test]
fn ast_parse_and_tokenize_take_sources_up_to_their_caps() {
    // Each parses in one native step, so its source is capped where the
    // worst step stays under 50 ms (docs/spec.md §5): at the cap it runs,
    // one byte past it raises MemoryError.
    let mut cases = vec![(
        "import tokenize\n_ = tokenize._tokens('a\\n' * 32768{})",
        "tokenize source larger than 64 KiB",
    )];
    if cfg!(feature = "mod-ast") {
        cases.push((
            "import ast\n_ = ast.parse('a\\n' * 16384{})",
            "ast.parse() source larger than 32 KiB",
        ));
    }
    for (src, message) in cases {
        let (p, _) = run(&src.replace("{}", ""));
        assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
        let (p, _) = run(&src.replace("{}", " + ' '"));
        let e = error_of(&p);
        assert_eq!(
            (e.type_name.as_str(), e.message.as_str()),
            ("MemoryError", message)
        );
    }
}

#[test]
fn unicodedata_normalizes_runs_up_to_their_cap() {
    // A run with no normalization slice boundary is normalized in one
    // native step, so it is capped at 65,536 chars after decomposition
    // (docs/user-guide.md): at the cap it runs, one char past it raises
    // MemoryError, in every form and in `is_normalized`.
    if !cfg!(feature = "mod-unicodedata") {
        return;
    }
    for f in ["'NFC'", "'NFD'", "'NFKC'", "'NFKD'"] {
        for call in ["normalize", "is_normalized"] {
            let src = format!(
                "import unicodedata\n_ = unicodedata.{call}({f}, 'ab ' * 5000 + 'x' + '\\u0301' * 65_535{{}} + ' y')"
            );
            let (p, _) = run(&src.replace("{}", ""));
            assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
            let (p, _) = run(&src.replace("{}", " + '\\u0316'"));
            let e = error_of(&p);
            assert_eq!(
                (e.type_name.as_str(), e.message.as_str()),
                (
                    "MemoryError",
                    "unicodedata: more than 65536 characters with no normalization boundary \
                     (micro-monty normalizes such a run at once)"
                ),
                "{src}"
            );
        }
    }
}

#[test]
fn symbol_table_and_compiler_errors_match_cpython() {
    // (source, message, line, 1-based column) as CPython 3.14.7's
    // `compile(source, '<s>', 'exec')` reports them; all SyntaxError.
    let engine = Engine::new(EngineConfig::default());
    for (src, msg, line, col) in [
        ("x = *a", "can't use starred expression here", 1, 5),
        (
            "[x := 1 for x in range(3)]",
            "assignment expression cannot rebind comprehension iteration variable 'x'",
            1,
            2,
        ),
        (
            "[x for y in [1] for x in (w := [1])]",
            "assignment expression cannot be used in a comprehension iterable expression",
            1,
            27,
        ),
        (
            "[x for x in (lambda: (y := [3]))()]",
            "assignment expression cannot be used in a comprehension iterable expression",
            1,
            23,
        ),
        (
            "def f():\n    from os import *",
            "import * only allowed at module level",
            2,
            20,
        ),
        (
            "[(yield) for x in y]",
            "'yield' inside list comprehension",
            1,
            3,
        ),
        (
            "{(yield) for x in y}",
            "'yield' inside set comprehension",
            1,
            3,
        ),
        (
            "{(yield): 1 for x in y}",
            "'yield' inside dict comprehension",
            1,
            3,
        ),
        (
            "((yield) for x in y)",
            "'yield' inside generator expression",
            1,
            3,
        ),
        ("with (yield): pass", "'yield' outside function", 1, 7),
    ] {
        let Err(err) = engine.compile(src, "<s>") else {
            panic!("{src:?}: compiled");
        };
        assert_eq!(
            (
                err.type_name.as_str(),
                err.message.as_str(),
                err.line,
                err.column + 1
            ),
            ("SyntaxError", msg, line, col),
            "{src:?}"
        );
    }
}

#[test]
fn oversized_ints_render_as_bit_counts_only_when_asked_and_never_hang_the_host() {
    let engine = Engine::new(EngineConfig::default());
    let code = engine
        .compile("big = 1 << 8_000_000\n[big, -big]", "t.py")
        .expect("compiles");
    let mut session = Session::new(&engine).expect("session");
    // CPython's behaviour by default.
    assert_eq!(error_of(&session.start(&code)).type_name, "ValueError");
    session.set_int_bits_text(true);
    let Progress::Complete(Some(c)) = session.start(&code) else {
        panic!("completes")
    };
    assert_eq!(c.repr, "[<int with 8000001 bits>, <int with 8000001 bits>]");
    assert_eq!((c.str, c.value), (c.repr.clone(), None));
    // A host call refuses it with CPython's limit instead of converting
    // 2.4 million digits.
    session
        .define(&HostModule::untyped("", &["f"]))
        .expect("defined");
    let call = engine.compile("f(big)", "t.py").expect("compiles");
    let e = error_of(&session.start(&call)).clone();
    assert_eq!(e.type_name, "ValueError");
    assert!(
        e.message.starts_with("Exceeds the limit (4300 digits)"),
        "{e:?}"
    );
}

#[test]
fn a_forged_dir_scope_reads_only_the_callers_own_registers() {
    // `<scope>` is the hidden keyword a compiled zero-argument `dir()`
    // passes; passed by hand with more names than the frame has registers,
    // it must stop at the caller's frame instead of reading past it.
    let (p, out) = run(
        "def f():\n    a = 1\n    return dir(**{'<scope>': ('a', 'b') * 300})\nprint(f())\n\
         print(dir(**{'<scope>': ('x',) * 300}), dir(**{'<scope>': {'k': 1}}))\n",
    );
    assert!(matches!(p, Progress::Complete(_)), "{p:?}\n{out}");
    assert!(out.starts_with("['a'"), "{out}");
    assert!(out.ends_with("['k']\n"), "{out}");
}

#[test]
fn gc_stress_collects_at_every_safepoint_without_changing_results() {
    let src = "def gen(n):\n    for i in range(n):\n        yield [i, str(i)] * 2\n\
               class A:\n    def __init__(self, x):\n        self.x = {'k': [x]}\n    def __repr__(self):\n        return f'A({self.x})'\n\
               d = {str(i): (i, [i]) for i in range(300)}\n\
               r = [sum(len(x) for x in gen(50)), d['7'], sorted([A(i) for i in range(5)], key=lambda a: -a.x['k'][0])]\nr";
    let engine = Engine::new(EngineConfig::default());
    let code = engine.compile(src, "t.py").expect("compiles");
    let mut reprs = Vec::new();
    for stress in [false, true] {
        let mut session = Session::new(&engine).expect("session");
        session.set_gc_stress(stress);
        let Progress::Complete(Some(c)) = session.start(&code) else {
            panic!("completes")
        };
        reprs.push(c.repr);
    }
    assert_eq!(reprs[0], reprs[1]);
}

#[test]
fn forged_re_programs_and_states_and_hash_states_are_errors() {
    // `re._exec` and the `hashlib` natives take whatever `bytes` a guest
    // passes as a compiled program, a saved run or a hash state. A program
    // whose group count was zeroed panicked at re.rs:560 ("the len is 1
    // but the index is 1"), and a SHAKE-128 state with a full buffer at
    // hashlib.rs:384. Every word of a few programs and saved runs, and
    // every header byte of each hash state, is forged here: each call
    // must run or raise, never panic or overflow (a debug build checks
    // arithmetic too).
    let src = r"
import re, hashlib
seen = {}
def call(f, *args):
    try:
        f(*args)
        k = 'ran'
    except (SystemError, TypeError, OverflowError, MemoryError) as e:
        k = type(e).__name__
    seen[k] = seen.get(k, 0) + 1
def forge(b, i, v):
    b = bytearray(b)
    b[4 * i:4 * i + 4] = v.to_bytes(4, 'little')
    return bytes(b)
words = (0, 1, 3, 8, 255, 2 ** 31, 2 ** 32 - 1)
for pat, s in [('ab?|c', 'xab c'), ('(a)(b)?|[^\\W\\d]{2,}?', 'xab \xe9e'), (r'(?P<x>\w+)\s(?P=x)', 'ab ab'),
               ('(?<=a)[b-d]{2,}?|(?>e+)f|(a)?(?(1)x|y)|(?i:k)*+', 'abcd eef y')]:
    p = re.compile(pat)
    n = len(p._code) // 4
    for i in range(n):
        for v in words + (i, i + 1, n):
            for mode in (2, 5, 9, 13):
                call(re._exec, forge(p._code, i, v), s, 0, 100, mode, 0, [None, [], False, 0, 0, p, 0, 100])
for pat, s in [('(a|\xe9)*?(b)c', 'a\xe9' * 40000), ('(a+)+b', 'a' * 40)]:
    p = re.compile(pat)
    st = [None, [], False, 0, 0, p, 0, 10 ** 9]
    re._exec(p._code, s, 0, 10 ** 9, 13, 0, st)
    saved = st[0]
    n = len(saved) // 4
    for i in range(0, n, 1 if n < 40 else 5):
        for v in (0, 1, 3, 2 ** 32 - 1, len(s.encode()) + 1):
            call(re._exec, p._code, s, 0, 10 ** 9, 13, 1, [forge(saved, i, v), [], False, 0, 0, p, 0, 10 ** 9])
    for cut in (0, 4, 36, len(saved) - 4):
        call(re._exec, p._code, s, 0, 10 ** 9, 13, 1, [saved[:cut], [], False, 0, 0, p, 0, 10 ** 9])
    for hint in ((5, 0), (0, 5), (2, 3), (len(s), 0)):
        call(re._exec, p._code, s, 0, 10 ** 9, 13, 1, [saved, [], False, *hint, p, 0, 10 ** 9])
    if s[1] == '\xe9':
        # A position inside a char (byte 2 is in the first 'é'), and a pc
        # on an operand: refused before the run goes on.
        for i, v in ((5, 2), (4, 5)):
            try:
                re._exec(p._code, s, 0, 10 ** 9, 13, 1, [forge(saved, i, v), [], False, 0, 0, p, 0, 10 ** 9])
            except SystemError as e:
                print(e)
for name in ('md5', 'sha1', 'sha256', 'sha512', 'sha3_256', 'shake_128', 'blake2b', 'blake2s'):
    h = hashlib.new(name)
    h.update(b'x' * 100)
    for i in range(21):
        for v in (0, 1, 64, 127, 128, 136, 168, 200, 255):
            st = bytearray(h._st)
            st[i] = v
            if i >= 5:
                st[5:21] = b'\xff' * 16
            st = bytes(st)
            call(hashlib._h_update, st, b'y' * 300)
            call(hashlib._h_digest, st, 16)
            call(hashlib._h_copy, st)
print(sorted(seen))
";
    let (p, out) = run(src);
    assert!(matches!(p, Progress::Complete(_)), "{p:?}\n{out}");
    assert_eq!(
        out,
        "re: corrupt saved state\nre: corrupt saved state\n\
         ['MemoryError', 'OverflowError', 'SystemError', 'TypeError', 'ran']\n"
    );
}

/// Two rounds of big set operations near the default 256 MiB limit,
/// where every round's sets are garbage by the next: Python 3.14 peaks
/// at 279 MiB here. Writing a big set's new hashes in steps of their own
/// before `a |= b` rebuilt into them delayed the rebuild: the collection
/// that ended during it kept the old 32 MiB table live, and round 2
/// raised `MemoryError` at `b = set(...)` for each of these sizes [M].
#[test]
fn two_rounds_of_big_set_operations_fit_the_default_limit() {
    for n in [631_000_u64, 656_250, 750_000, 900_000] {
        let src = format!(
            "N = {n}\nt = 0\nfor r in range(2):\n    a = set(range(N))\n    b = set(range(N // 2, N + N // 2))\n    c = a & b\n    t += len(c)\n    d = dict(zip(range(N), range(N)))\n    t += len(d)\n    a |= b\n    t += len(a)\nprint(t)\n"
        );
        let (p, out) = run(&src);
        assert!(matches!(p, Progress::Complete(_)), "N = {n}: {p:?}");
        // Each round: |a & b| = N - N // 2, |d| = N, |a | b| = N + N // 2.
        let want = 2 * ((n - n / 2) + n + (n + n / 2));
        assert_eq!(out, format!("{want}\n"), "N = {n}");
    }
}

/// A copy beside garbage not yet collected: with a quarter of the default
/// limit live, two copies made and dropped, the third raised `MemoryError`
/// where Python 3.14 runs it [M]. The copies of mutable types, the
/// constructors and slices, and a chunked set or dict operation whose new
/// table did not fit were not run again after a collection, and a retry
/// needed 1 MiB allocated since the last collection, which dropping a
/// value does not allocate. Each now waits for one collection and goes on
/// (`Vm::oom_retry`, `Vm::oom_defer`).
#[test]
fn a_copy_beside_dropped_copies_waits_for_a_collection() {
    let set = "x = set(range(2_000_000))";
    let dict = "x = dict.fromkeys(range(2_000_000))";
    let list = "x = list(range(8_000_000))";
    for (setup, op) in [
        (set, "x.copy()"),
        (set, "x | {1}"),
        (set, "frozenset(x)"),
        (set, "set(x)"),
        (dict, "x.copy()"),
        (dict, "x | {}"),
        (dict, "dict.fromkeys(x)"),
        (dict, "dict(x)"),
        (dict, "{**x}"),
        (list, "x.copy()"),
        (list, "[y for y in x]"),
        (list, "x[:]"),
        (list, "list(x)"),
        (list, "tuple(x)"),
        (list, "x + []"),
        (list, "[*x]"),
    ] {
        let src = format!(
            "{setup}\nfor _ in range(3):\n    a = {op}\n    b = {op}\n    a = b = None\n    c = {op}\n    c = None\nprint('ok')\n"
        );
        let (p, out) = run(&src);
        assert!(matches!(p, Progress::Complete(_)), "{op}: {p:?}\n{out}");
        assert_eq!(out, "ok\n", "{op}");
    }
}

/// The same beside a dropped 3M-element set, for keys with a user
/// `__hash__`: their prelude twins insert each key with its hash
/// (`_set_insert`, `_dict_insert`), and a table that did not grow raised
/// `MemoryError` where Python 3.14 runs it [M]. The insert now runs again
/// after a collection, without calling `__hash__` again.
#[test]
fn a_user_hashed_insert_waits_for_a_collection() {
    let src = "\
calls = [0]
class K:
    def __init__(self, v):
        self.v = v
    def __hash__(self):
        calls[0] += 1
        return self.v
    def __eq__(self, o):
        return self.v is o.v
xs = [K(i) for i in range(800_000)]
for op in range(4):
    g = set(range(3_000_000))
    g = None
    calls[0] = 0
    if op == 0:
        s = {x: 1 for x in xs}
    elif op == 1:
        s = {x for x in xs}
    elif op == 2:
        s = set(xs)
    else:
        s = dict.fromkeys(xs)
    assert len(s) == 800_000
    assert calls[0] == 800_000
    s = None
print('ok')
";
    let (p, out) = run(src);
    assert!(matches!(p, Progress::Complete(_)), "{p:?}\n{out}");
    assert_eq!(out, "ok\n");
}

/// A set's new table whose keys fit and whose hashes do not, beside a
/// kept value at a 64 MiB limit: the keys, dropped, counted as allocated
/// since the collection the step waited for, so every failure deferred
/// again and the step collected until its fuel ran out [M]. It raises
/// `MemoryError` after one collection (`Vm::oom_defer`).
#[test]
fn a_table_that_never_fits_raises_after_one_collection() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(64 << 20)));
    let code = engine
        .compile(
            "keep = bytearray(30_000_000)\ns = set(range(750_000))\n",
            "test.py",
        )
        .expect("compiles");
    let mut s = Session::new(&engine).expect("session");
    s.set_fuel_total(Some(1 << 30));
    let p = s.start(&code);
    assert_eq!(error_of(&p).type_name, "MemoryError", "{p:?}");
}

/// A result of 64 KiB or more with half the memory left allocated since
/// the last collection asks for a full one (`Vm::collect_for_result`).
/// Near the limit that is little, so a loop of 70 KB results forced a
/// collection each for about 1,100 units [M, reviewer]. With less than
/// 1/64 of the limit allocated since the last collection, the step now
/// pays as a retry after `MemoryError` does, at least 2^20 units.
#[test]
fn a_result_collection_near_the_limit_is_charged() {
    let spent = |n: usize| {
        let engine =
            Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(64 << 20)));
        let src = format!(
            "import gc
keep = [[i] for i in range(300_000)]
pad = []
try:
    while True:
        pad.append(bytes(1 << 20))
except MemoryError:
    pass
try:
    while True:
        pad.append('y' * 70001)
except MemoryError:
    pass
del pad[-3:]
gc.collect()
s = 'x' * 70000
t = None
for i in range({n}):
    t = s + 'y'
"
        );
        let code = engine.compile(&src, "test.py").expect("compiles");
        let mut s = Session::new(&engine).expect("session");
        s.set_fuel_total(Some(1 << 40));
        let p = s.start(&code);
        assert_eq!(p, Progress::Complete(None), "{n}");
        (1 << 40) - s.fuel_total().expect("a total")
    };
    let (none, some) = (spent(0), spent(20));
    assert!(
        some - none >= 20 * ((1 << 20) - (1 << 12)),
        "20 results: {some} against {none}"
    );
}

/// A retry after `MemoryError` waits for a full collection, and charges
/// the failing operation for it (`Vm::collect_for_retry`), so a guest that
/// catches `MemoryError` in a loop cannot have collections run for free:
/// each failing `*` beside 225 MB live costs at least 2^20 units more than
/// raising `MemoryError` itself. A request past the largest object raises
/// at once, uncharged.
#[test]
fn a_retry_charges_its_collection() {
    let spent = |body: &str| {
        let engine = Engine::new(EngineConfig::new());
        let code = engine
            .compile(
                &format!(
                    "keep = [bytearray(45_000_000) for _ in range(5)]\nfor _ in range(10):\n    try:\n        {body}\n    except MemoryError:\n        pass\n"
                ),
                "test.py",
            )
            .expect("compiles");
        let mut s = Session::new(&engine).expect("session");
        s.set_fuel_total(Some(1 << 40));
        let p = s.start(&code);
        assert_eq!(p, Progress::Complete(None), "{body}");
        (1 << 40) - s.fuel_total().expect("a total")
    };
    let failed = spent("b'x' * 60_000_000");
    let too_big = spent("b'x' * 300_000_000");
    let raised = spent("raise MemoryError");
    assert!(too_big < raised + (1 << 20), "{too_big} against {raised}");
    assert!(
        failed >= raised + 10 * (1 << 20),
        "{failed} against {raised}"
    );
}
