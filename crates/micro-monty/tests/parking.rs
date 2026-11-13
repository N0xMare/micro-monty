//! Pause-anywhere: parks requested from another thread, by fuel and by
//! deadline, at any depth (nested calls, coroutines and tasks,
//! comprehensions, `for` loops), resume exactly where they stopped, and
//! leave output identical to an unparked run. Every `conformance/own` case
//! is run parked every 1,000 fuel units.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    Engine, EngineConfig, HostModule, ParkReason, Progress, Resume, Session, Stream,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Instant,
};

/// A session whose output (tagged by stream) collects into `out`.
fn session(engine: &Engine, out: &Arc<Mutex<String>>) -> Session {
    let mut session = Session::new(engine).expect("session");
    let sink = Arc::clone(out);
    session.set_output_streams(Box::new(move |stream, s: &str| {
        let mut o = sink.lock().expect("output lock");
        if stream == Stream::Stderr {
            o.push_str("[err]");
        }
        o.push_str(s);
    }));
    session
}

/// The final event of a run, comparable across runs.
fn outcome(p: &Progress) -> String {
    match p {
        Progress::Complete(c) => format!("complete {:?}", c.as_ref().map(|c| &c.repr)),
        Progress::Error(e) => format!("error\n{}", e.traceback),
        other => panic!("unexpected {other:?}"),
    }
}

/// `f` on every item, spread over every core; the results in item order.
fn on_every_core<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let result = f(item);
                    *results[i].lock().expect("result lock") = Some(result);
                }
            });
        }
    });
    results
        .into_iter()
        .map(|r| {
            r.into_inner()
                .expect("result lock")
                .expect("every item ran")
        })
        .collect()
}

/// Runs `cells` in one session, unparked; returns output and outcomes.
fn run_plain(cells: &[&str], filename: &str) -> (String, Vec<String>) {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let mut outcomes = Vec::new();
    for src in cells {
        let code = engine.compile(src, filename).expect("compiles");
        outcomes.push(outcome(&s.start(&code)));
    }
    let text = out.lock().expect("output lock").clone();
    (text, outcomes)
}

/// Runs `cells` parking every `fuel` units; returns output, outcomes and
/// the number of parks.
fn run_parked(cells: &[&str], filename: &str, fuel: u64) -> (String, Vec<String>, usize) {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let mut outcomes = Vec::new();
    let mut parks = 0;
    for src in cells {
        let code = engine.compile(src, filename).expect("compiles");
        s.set_fuel(Some(fuel));
        let mut p = s.start(&code);
        while let Progress::Parked { reason } = p {
            assert_eq!(reason, ParkReason::Fuel);
            parks += 1;
            s.set_fuel(Some(fuel));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        outcomes.push(outcome(&p));
    }
    s.set_fuel(None);
    let text = out.lock().expect("output lock").clone();
    (text, outcomes, parks)
}

#[test]
fn a_park_requested_from_another_thread_stops_a_nested_loop() {
    // The cell prints a mark inside the first inner loop; the output
    // callback hands it to another thread and waits until that thread has
    // requested the park, so the request is set while the loop still has
    // most of its iterations to run, however loaded the machine is.
    let src = "\
def work(n):
    def inner(k, first):
        t = 0
        for i in range(k):
            if first and i == 1000:
                print('looping')
            t = (t + i * i) % 1000003
        return t
    return [inner(n, j == 0) for j in range(3)]
print(work(60000))
";
    let (plain, plain_outcome) = run_plain(&[src], "park.py");
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = Session::new(&engine).expect("session");
    let handle = s.park_handle();
    let (tx, rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let sink = Arc::clone(&out);
    s.set_output(Box::new(move |text: &str| {
        sink.lock().expect("output lock").push_str(text);
        if text == "looping\n" {
            let _ = tx.send(());
            let _ = done_rx.recv();
        }
    }));
    let requester = std::thread::spawn(move || {
        // A bound against hanging if the mark never comes (the assertions
        // below then fail), not a timing assumption.
        if rx.recv_timeout(std::time::Duration::from_secs(60)).is_ok() {
            handle.request();
        }
        let _ = done_tx.send(());
    });
    let code = engine.compile(src, "park.py").expect("compiles");
    let p = s.start(&code);
    requester.join().expect("joins");
    assert_eq!(
        p,
        Progress::Parked {
            reason: ParkReason::Requested
        }
    );
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "looping\n",
        "parked inside the loop"
    );
    let p = s.resume(Resume::Continue).expect("resumes");
    assert_eq!(out.lock().expect("output lock").as_str(), plain);
    assert_eq!(vec![outcome(&p)], plain_outcome);
}

#[test]
fn a_deadline_parks_and_resume_continues() {
    let src = "t = 0\nfor i in range(200000):\n    t += i\nt";
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let code = engine.compile(src, "deadline.py").expect("compiles");
    s.set_deadline(Some(Instant::now()));
    let p = s.start(&code);
    assert_eq!(
        p,
        Progress::Parked {
            reason: ParkReason::Deadline
        }
    );
    // One-shot: resuming without a new deadline runs to the end.
    let p = s.resume(Resume::Continue).expect("resumes");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(c.repr, "19999900000");
}

#[test]
fn native_work_observes_control_without_a_fuel_limit() {
    for (setup, operation, check) in [
        (
            "data = ['x'] * 5000",
            "result = ''.join(data)",
            "assert result == 'x' * 5000",
        ),
        (
            "data = list(range(4095, -1, -1))",
            "data.sort()",
            "assert data == list(range(4096))",
        ),
        (
            "import json\ndata = ['x' * 5000]",
            "result = json.dumps(data)",
            "assert len(result) == 5004 and result.count('x') == 5000",
        ),
    ] {
        for reason in [ParkReason::Requested, ParkReason::Deadline] {
            let engine = Engine::new(EngineConfig::default());
            let mut s = Session::new(&engine).expect("session");
            s.define(&HostModule::untyped("", &["before"]))
                .expect("host function");
            let code = engine
                .compile(
                    &format!("{setup}\nbefore()\n{operation}\n17"),
                    "native-control.py",
                )
                .expect("compiles");
            assert!(matches!(s.start(&code), Progress::HostCall { .. }));
            assert_eq!(s.fuel(), None);
            if reason == ParkReason::Requested {
                s.request_park();
            } else {
                s.set_deadline_with(Box::new(|| 1), 1);
            }
            assert_eq!(
                s.resume(Resume::Value(micro_monty::HostValue::None))
                    .expect("host reply"),
                Progress::Parked { reason },
                "{operation}: {reason:?}"
            );
            let snapshot = s.snapshot().expect("snapshot");
            s = engine.restore(&snapshot).expect("restore");
            assert_eq!(
                outcome(&s.resume(Resume::Continue).expect("resume")),
                "complete Some(\"17\")"
            );
            let check = engine.compile(check, "check.py").expect("compiles");
            assert_eq!(s.start(&check), Progress::Complete(None));
        }
    }
}

const CELLS: [&str; 4] = [
    "\
import asyncio, sys
log = []
def fib(n):
    return n if n < 2 else fib(n - 1) + fib(n - 2)
async def worker(name, n):
    total = 0
    for i in range(n):
        total += fib(i % 15)
        if i % 50 == 0:
            await asyncio.sleep(0)
            log.append(name)
    return name, total
",
    "\
async def main():
    rs = await asyncio.gather(worker('a', 300), worker('b', 200), worker('c', 250))
    nested = await worker('d', 120)
    return rs, nested
print(asyncio.run(main()))
print(''.join(log), file=sys.stderr)
",
    "\
table = {k: [fib(j) for j in range(k)] for k in range(18)}
words = [w.upper() for w in 'the quick brown fox'.split() for _ in range(40)]
print(sum(len(v) for v in table.values()), len(words), sum(table[17]))
r = await asyncio.gather(*(worker(str(i), 60) for i in range(4)))
print(r)
",
    "\
def deep(n):
    return 0 if n == 0 else 1 + deep(n - 1)
items = list(range(3000))
acc = 0
for x in items:
    acc += deep(x % 40)
print(acc, len(log))
try:
    deep(10000)
except RecursionError as e:
    print('recursion', e)
acc
",
];

#[test]
fn fuel_parks_at_any_depth_across_cells_and_matches_an_unparked_run() {
    let (plain, plain_outcomes) = run_plain(&CELLS, "cells.py");
    assert!(plain.contains("[err]"), "{plain}");
    for (fuel, min_parks) in [(1, 500), (1_000, 100), (7_777, 10), (100_000, 1)] {
        let (parked, outcomes, parks) = run_parked(&CELLS, "cells.py", fuel);
        assert!(parks >= min_parks, "fuel {fuel}: parked only {parks} times");
        assert_eq!(parked, plain, "fuel {fuel}");
        assert_eq!(outcomes, plain_outcomes, "fuel {fuel}");
    }
}

#[test]
fn dir_parks_inside_a_user_dunder_and_in_function_scope() {
    // A user `__dir__` runs as prelude bytecode in the same loop, so a
    // park lands inside it; zero-argument `dir()` reads its caller's
    // registers, which must be the resumed frame's.
    let src = "class C:\n    def __dir__(self):\n        return [str(i % 7) for i in range(60)]\n\
               def f(a, b=2):\n    c = [dir(C()) for _ in range(3)]\n    if a:\n        d = 1\n    return dir(), c[0]\n\
               print(f(0), f(1))\n";
    let (plain, plain_outcomes) = run_plain(&[src], "dir.py");
    assert!(plain.contains("(['a', 'b', 'c'], ['0', '0'"), "{plain}");
    assert!(plain.contains("(['a', 'b', 'c', 'd'], ['0'"), "{plain}");
    let (parked, outcomes, parks) = run_parked(&[src], "dir.py", 1);
    assert!(parks >= 100, "parked only {parks} times");
    assert_eq!((parked, outcomes), (plain, plain_outcomes));
}

#[test]
fn a_cell_run_in_small_fuel_slices_still_collects_its_garbage() {
    // With slices of at most 256 units every safepoint check ends in a fuel
    // park; the collection due there must still run, or 12 MB of garbage
    // fills an 8 MiB limit that an unparked run never comes near.
    let engine = Engine::new(
        EngineConfig::new().with_limits(micro_monty::Limits::new().with_memory(8 << 20)),
    );
    let code = engine
        .compile(
            "for i in range(30000):\n    x = [i] * 50\nprint('done')\n",
            "garbage.py",
        )
        .expect("compiles");
    for fuel in [7, 256] {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        s.set_fuel(Some(fuel));
        let mut p = s.start(&code);
        while let Progress::Parked { .. } = p {
            s.set_fuel(Some(fuel));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        assert!(
            outcome(&p).starts_with("complete"),
            "fuel {fuel}: {}",
            outcome(&p)
        );
        assert_eq!(*out.lock().expect("output lock"), "done\n", "fuel {fuel}");
    }
}

#[test]
fn every_own_case_matches_an_unparked_run_when_parked_every_1000_fuel() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/own");
    let mut cases: Vec<_> = std::fs::read_dir(&root)
        .expect("conformance/own exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .collect();
    cases.sort();
    assert!(cases.len() >= 64, "{} own cases", cases.len());
    // Every 1,000 units, and at every safepoint; the cases on every core
    // (about 20 minutes on one in a debug build), checked in name order.
    for fuel in [1_000, 1] {
        let mut total_parks = 0;
        let runs = on_every_core(&cases, |case| {
            let src = std::fs::read_to_string(case).expect("readable");
            let name = case.to_string_lossy();
            (run_plain(&[&src], &name), run_parked(&[&src], &name, fuel))
        });
        for (case, ((plain, plain_outcome), (parked, outcome, parks))) in cases.iter().zip(runs) {
            let name = case.to_string_lossy();
            assert_eq!(parked, plain, "{name}: output differs parked every {fuel}");
            assert_eq!(
                outcome, plain_outcome,
                "{name}: outcome differs parked every {fuel}"
            );
            total_parks += parks;
        }
        eprintln!("own cases parked {total_parks} times at fuel {fuel}");
        assert!(total_parks > 0, "never parked at fuel {fuel}");
    }
}

#[test]
fn collecting_a_huge_iterable_stops_promptly_and_charges_fuel() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    // A range's length is checked against the memory limit before any item
    // is collected (this used to gather 2^26 items first, for seconds).
    s.set_fuel(Some(10_000));
    let t = Instant::now();
    let code = engine
        .compile("xs = list(range(10**9))\n", "huge.py")
        .expect("compiles");
    let p = s.start(&code);
    let Progress::Error(e) = &p else {
        panic!("{p:?}")
    };
    assert!(e.traceback.contains("MemoryError"), "{}", e.traceback);
    assert!(t.elapsed().as_secs() < 2, "took {:?}", t.elapsed());
    // An iterator of unknown length is charged per item: 200,000 items from
    // one native call exhaust 10,000 units, so the run parks at the next
    // safepoint (the loop), before `print`.
    s.set_fuel(Some(10_000));
    let code = engine
        .compile(
            "xs = list(zip(range(200_000), range(200_000)))\nfor _ in range(2):\n    pass\nprint(len(xs))\n",
            "zip.py",
        )
        .expect("compiles");
    let p = s.start(&code);
    assert_eq!(
        p,
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    );
    assert_eq!(out.lock().expect("output lock").as_str(), "");
    s.set_fuel(None);
    let p = s.resume(Resume::Continue).expect("resumes");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(out.lock().expect("output lock").as_str(), "200000\n");
}

#[test]
fn indexing_a_long_non_ascii_str_charges_its_walks_and_stays_linear() {
    // `s[i]` on a non-ASCII str walks to the i-th char (fuzz finding
    // run_source-slow-strindex-58d44444) from the nearest of the string's
    // ends and the positions found lately, charging the bytes walked.
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let fuel = |s: &mut Session, src: &str| {
        s.set_fuel(Some(u64::MAX / 2));
        let p = s.start(&engine.compile(src, "idx.py").expect("compiles"));
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        u64::MAX / 2 - s.fuel().expect("fuel")
    };
    // 200,000 chars, 400,000 bytes, in both.
    let setup = "e = 'é' * 200_000\na = 'a' * 200_000\nn = 200_000\n";
    fuel(&mut s, setup);
    // The first walk to the middle is charged, one unit per 64 bytes.
    let far = fuel(&mut s, "c = e[100_000]\n");
    assert!(far >= 100_000 * 2 / 64, "spent {far}");
    // Forward, backward, from both ends at once, and sliding slices: about
    // the fuel of an ASCII string. Each collection forgets the positions,
    // so the next walk starts from an end again; collections follow
    // allocation, so that stays in proportion (7% here). An O(i) walk per
    // index would cost about 400 times the ASCII fuel.
    let loops = "for i in range(n):\n    c = s[i]\n\
                 for i in range(n - 1, -1, -1):\n    c = s[i]\n\
                 for i in range(n // 2):\n    c = s[i] + s[-1 - i]\n\
                 for i in range(n):\n    c = s[i:i + 3]\n\
                 print(c, s[n // 2], s[-1])\n";
    let ascii = fuel(&mut s, &format!("s = a\n{loops}"));
    let non_ascii = fuel(&mut s, &format!("s = e\n{loops}"));
    assert_eq!(out.lock().expect("output lock").as_str(), "a a a\né é é\n");
    assert!(
        non_ascii < ascii * 3 / 2,
        "ASCII {ascii}, non-ASCII {non_ascii}"
    );
}

#[test]
fn membership_scans_charge_fuel_and_an_endless_iterator_parks() {
    // Fuzz finding vm_bytecode-slow-in-08c658ac: `x in seq` compared item
    // by item for no fuel, and `x in iterator` ran natively to the end, so
    // `-1 in iter(range(10**12))` ignored fuel, deadlines and cancel.
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let setup = "x = [0] * 1_000_000\n";
    let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    s.set_fuel(Some(u64::MAX / 2));
    let src = "print(1 in x, x.count(1), (0, 1) in {0: 1}.items(), 2 in {0: 2}.values())\n";
    let p = s.start(&engine.compile(src, "scan.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let spent = u64::MAX / 2 - s.fuel().expect("fuel");
    assert!(spent > 2_000_000, "spent {spent}");
    // An endless iterator parks on fuel, resumes where it stopped, and
    // parks on a deadline with no fuel limit.
    let src = "it = iter(range(10**12))\nprint(40_000 in it, next(it))\nprint(-1 in it)\n";
    let code = engine.compile(src, "iter.py").expect("compiles");
    s.set_fuel(Some(100_000));
    let mut p = s.start(&code);
    let mut parks = 0;
    while matches!(
        p,
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    ) && parks < 10
    {
        parks += 1;
        s.set_fuel(Some(100_000));
        p = s.resume(Resume::Continue).expect("resumes");
    }
    assert!(
        matches!(
            p,
            Progress::Parked {
                reason: ParkReason::Fuel
            }
        ),
        "{p:?}"
    );
    s.set_fuel(None);
    s.set_deadline(Some(Instant::now() + std::time::Duration::from_millis(50)));
    let t = Instant::now();
    let p = s.resume(Resume::Continue).expect("resumes");
    assert!(
        matches!(
            p,
            Progress::Parked {
                reason: ParkReason::Deadline
            }
        ),
        "{p:?}"
    );
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "False 0 True True\nTrue 40001\n"
    );
}

#[test]
fn folds_over_an_endless_iterator_park_on_fuel_and_on_a_deadline() {
    // Fuel-audit finding: `any` and `all` ran a native iterator to its end
    // for no fuel, so `all(range(1, 10**18))` ignored fuel, deadlines and
    // cancel. They charge per item and go on in steps (`nstep::op::FOLD`)
    // or in the prelude. `min`, `max` and `sum` collected an iterator
    // first: over a long range that was a MemoryError, and over a 60M
    // char str one 2 s step; they fold in steps too.
    for op in [
        "all(range(1, 10**18))",
        "all(zip(range(10**18)))",
        "any(itertools.repeat(0))",
        "any(iter(itertools.repeat(0)))",
        "sum(iter(range(10**18)))",
        "sum(range(10**18), 0.5)",
        "max(iter(range(10**18)))",
        "min(range(10**18))",
    ] {
        let engine = Engine::new(EngineConfig::default());
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        let code = engine
            .compile(&format!("import itertools\nprint({op})\n"), "<endless>")
            .expect("compiles");
        s.set_fuel(Some(100_000));
        let mut p = s.start(&code);
        for _ in 0..3 {
            assert_eq!(
                p,
                Progress::Parked {
                    reason: ParkReason::Fuel
                },
                "{op}"
            );
            s.set_fuel(Some(100_000));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        let snapshot = s.snapshot().expect("snapshot");
        s = engine.restore(&snapshot).expect("restore");
        s.set_fuel(None);
        s.set_deadline(Some(Instant::now() + std::time::Duration::from_millis(50)));
        let t = Instant::now();
        let p = s.resume(Resume::Continue).expect("resumes");
        assert_eq!(
            p,
            Progress::Parked {
                reason: ParkReason::Deadline
            },
            "{op}"
        );
        assert!(t.elapsed().as_secs() < 2, "{op}: took {:?}", t.elapsed());
    }
}

/// The fuel slice [`collecting_an_endless_iterator_parks_on_fuel_and_on_a_deadline`]
/// gives.
const SLICE: u64 = 20_000;
/// The most one of its slices may charge past [`SLICE`]: a chunk of the
/// collect (`COLLECT_CHUNK`, 65,536 items) charges 45,537 to 111,078 units
/// past it [M]; a chunk four times as large charges 274,917 and fails.
const OVERDRAFT: u64 = 3 << 16;

#[test]
fn collecting_an_endless_iterator_parks_on_fuel_and_on_a_deadline() {
    // Fuel-audit finding: `collect` charged per item but gathered a native
    // iterator in one step, so `list(enumerate(range(10**18)))` ran 0.5 s
    // to MemoryError (`dict(zip(...))` 0.7 s), deaf to fuel, deadlines and
    // cancel. Past a fixed chunk the prelude gathers the rest, parking
    // between chunks. (A generator, `map` included, always ran in Python.)
    for op in [
        "list(enumerate(range(10**18)))",
        "tuple(zip(range(10**18)))",
        "set(enumerate(range(10**18)))",
        "sorted(zip(range(10**18)), reverse=True)",
        "dict(zip(it, it))",
        "dict(enumerate(range(10**18)))",
        "dict.fromkeys(zip(range(10**18)))",
        "max(enumerate(range(10**18)))",
        "[*zip(range(10**18))]",
        "f(*enumerate(range(10**18)))",
        "(a, *b) = zip(range(10**18))",
        "frozenset(zip(range(10**18), range(10**18)))",
    ] {
        let engine = Engine::new(EngineConfig::default());
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        let src = format!("it = iter(range(10**18))\ndef f(*a): pass\nx = {op}\n");
        let code = engine.compile(&src, "<endless>").expect("compiles");
        let total = u64::MAX / 2;
        s.set_fuel_total(Some(total));
        let mut left = total;
        // What the slice just ended charged past the `given` units it had.
        let mut over = |s: &Session, given: u64| {
            let now = s.fuel_total().expect("a total");
            let over = (left - now).saturating_sub(given);
            left = now;
            over
        };
        s.set_fuel(Some(SLICE));
        let mut p = s.start(&code);
        for _ in 0..3 {
            assert_eq!(
                p,
                Progress::Parked {
                    reason: ParkReason::Fuel
                },
                "{op}"
            );
            // Fuel is the check, not time, so machine load cannot fail it:
            // a collect gathering past its chunk overdraws the slice (one
            // that runs on to MemoryError does not park at all).
            let o = over(&s, SLICE);
            assert!(
                o <= OVERDRAFT,
                "{op}: a slice charged {o} units past its {SLICE}"
            );
            s.set_fuel(Some(SLICE));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        let snapshot = s.snapshot().expect("snapshot");
        s = engine.restore(&snapshot).expect("restore");
        s.set_fuel(None);
        s.set_deadline(Some(Instant::now() + std::time::Duration::from_millis(50)));
        let p = s.resume(Resume::Continue).expect("resumes");
        // A collect deaf to the deadline runs on to MemoryError instead.
        assert_eq!(
            p,
            Progress::Parked {
                reason: ParkReason::Deadline
            },
            "{op}"
        );
    }
    // Parked in the middle of a collect, through a snapshot, the result is
    // an unparked run's.
    let src = "it = iter(range(200_000))\nprint(sum(sorted(zip(it, it), reverse=True)[0]), next(it, 'end'), len(bytes(reversed([1] * 70_000))))\n";
    let plain = run_plain(&[src], "chunks.py");
    let (text, outcomes, parks) = run_parked(&[src], "chunks.py", 30_000);
    assert!(parks > 3, "parked {parks} times");
    assert_eq!((text, outcomes), plain);
}

#[test]
fn native_work_over_long_data_charges_fuel_in_proportion() {
    // Fuel-audit findings: natives that walk guest data charged nothing, or
    // a constant, for work in proportion to its size. Each operation below
    // must spend at least one unit per 64 bytes or per item it walks (4 MB
    // texts, 300,000 items, a 1 MB int) on each pass over its operands and
    // its result, and big-int products and powers their quadratic share; it
    // spent next to nothing before. The rows after `big.bit_count()` each
    // fail with one charge site removed (mutation testing).
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let setup = "\
import binascii, json, re
a = 'a' * 4_000_000 + 'z'
b = 'a' * 4_000_000 + 'z'
ba, bb = a.encode(), b.encode()
L = list(range(300_000))
s1, s2 = set(L), set(L)
d1 = dict.fromkeys(L)
d2 = dict(d1)
big = (1 << 8_422_065) // 3
b1 = 7 ** 30_000
b2 = b1 * b1
";
    let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let mut spent = |src: &str| {
        s.set_fuel(Some(u64::MAX / 2));
        let p = s.start(&engine.compile(src, "op.py").expect("compiles"));
        assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
        u64::MAX / 2 - s.fuel().expect("fuel")
    };
    let text = 4_000_000 / 64;
    for (op, min) in [
        ("a == b", text),
        ("[a] == [b]", text),
        ("a < b", text),
        ("ba == bb", text),
        ("a.isalpha()", text),
        ("a.startswith(b)", text),
        ("'b' in a", text),
        ("hash(ba)", text),
        ("re.search('b', a)", text),
        ("s1 == s2", 300_000),
        ("s1 <= s2", 300_000),
        ("d1 == d2", 300_000),
        ("sum(L)", 300_000),
        ("max(L)", 300_000),
        ("big & big", 16_000),
        ("big.bit_count()", 16_000),
        ("binascii.hexlify(ba)", 3 * text),
        ("ba * 2", 3 * text),
        ("ba.decode()", 2 * text),
        ("ba.split(b'z')", 2 * text),
        ("b''.join([ba, ba])", 4 * text),
        ("int.from_bytes(ba, 'big')", 2 * text),
        // `max` walks the items; `[*L]` and its tuple copy them, a unit
        // per 8 items each, as `list.extend` and `tuple(L)` do.
        ("max(*L)", 375_000),
        ("dict.fromkeys(L)", 600_000),
        ("hex(big)", 48_000),
        ("big + big", 48_000),
        ("big << 64", 32_000),
        ("big >> 64", 32_000),
        ("b2 % b1", 100_000),
        ("b1 ** 3", 100_000),
        ("pow(b1, 3, b2)", 100_000),
        // Both tables are read.
        ("bytes.maketrans(ba, bb)", 2 * text),
    ] {
        let n = spent(&format!("x = {op}\n"));
        assert!(n >= min, "{op}: spent {n}, want at least {min}");
    }
    // `json.dumps` charges the text it adds, not all of it again every
    // 1,024 values (that was quadratic: 61.5M units for 1M ints).
    let n = spent("x = json.dumps(L * 4)\n");
    assert!((100_000..10_000_000).contains(&n), "json.dumps: spent {n}");
}

#[test]
fn class_regex_and_parse_error_work_charges_fuel_in_proportion() {
    // Fuel-audit sweep: work that charged nothing beyond its instruction.
    // `B.x = v` revisits every subclass (and was quadratic in them), class
    // creation reads the MRO, a shape scans its transitions, a regex set
    // is read item by item and every attempt clears the group registers
    // (a lookahead first, so the search cannot skip to an `x` or `y`),
    // `int`/`complex` scan the text before failing, and `str.replace`
    // pays for each match.
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let setup = "\
import re
class B: pass
subs = [type('S' + str(i), (B,), {}) for i in range(3000)]
deep = B
for i in range(500):
    deep = type('D' + str(i), (deep,), {})
class C: pass
keep = [C() for i in range(3000)]
for i, o in enumerate(keep):
    setattr(o, 'n' + str(i), 1)
digits = '1' * 1_000_000 + 'x'
wide = re.compile('[' + ''.join(chr(0x4e00 + i) for i in range(3000)) + ']')
groups = re.compile('(?=[xy])[xy]' + '(a)?' * 3000)
subject = 'a' * 100_000
";
    let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let mut spent = |src: &str| {
        s.set_fuel(Some(u64::MAX / 2));
        let p = s.start(&engine.compile(src, "op.py").expect("compiles"));
        assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
        u64::MAX / 2 - s.fuel().expect("fuel")
    };
    for (op, min) in [
        ("B.x = 1", 3000),
        ("type('E', (deep,), {})", 500),
        ("setattr(C(), 'n2999', 2)", 300),
        ("wide.findall(subject)", 1_000_000),
        ("groups.search(subject)", 1_000_000),
        (
            "try:\n    int(digits)\nexcept ValueError:\n    pass",
            15_000,
        ),
        (
            "try:\n    complex(digits)\nexcept ValueError:\n    pass",
            15_000,
        ),
        ("subject.replace('a', 'b')", 400_000),
    ] {
        let n = spent(&format!("{op}\n"));
        assert!(n >= min, "{op}: spent {n}, want at least {min}");
    }
}

#[test]
fn long_lines_and_tracebacks_stay_linear() {
    // Fuel-audit sweep: one native step was quadratic in its input. A
    // column on a non-ASCII line counted from the line start (`ast.parse`
    // and `tokenize` of one long line: over 80 s), each traceback frame
    // split the whole source, and each `strptime` alternative rescanned a
    // whitespace run (now mm-engine's `a_long_whitespace_run_is_scanned_once`:
    // its native is the bundled modules' own). Each slow case must stay
    // within a small factor of its control.
    let engine = Engine::new(EngineConfig::default());
    let time = |src: &str| {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        let code = engine.compile(src, "linear.py").expect("compiles");
        let t = Instant::now();
        let p = s.start(&code);
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        t.elapsed()
    };
    let lex = |item: &str| {
        format!(
            "import tokenize\nt = tokenize._tokens('x = [' + ', '.join([{item:?}] * 10_000) + ']')\n"
        )
    };
    let pad = "#".repeat(79) + "\n";
    let trace = |lines: usize| {
        format!(
            "import traceback\n{}def f(n):\n    if n == 0:\n        raise ValueError\n    f(n - 1)\ntry:\n    f(900)\nexcept ValueError:\n    s = traceback.format_exc()\n",
            pad.repeat(lines)
        )
    };
    for (what, slow, control) in [
        ("tokenize", lex("'é'"), lex("'e'")),
        ("traceback", trace(2_500), trace(1)),
    ] {
        // A run over the bound runs twice more and the least counts: load
        // (`cargo xtask check` runs the other tests beside it) can only
        // lengthen a run, never shorten the slow one.
        let (mut least, mut control_least) = (time(&slow), time(&control));
        for _ in 0..2 {
            if least < control_least * 4 + std::time::Duration::from_millis(500) {
                break;
            }
            least = least.min(time(&slow));
            control_least = control_least.min(time(&control));
        }
        assert!(
            least < control_least * 4 + std::time::Duration::from_millis(500),
            "{what}: {least:?} against {control_least:?}"
        );
    }
}

#[test]
fn straight_line_native_work_parks_on_fuel_and_on_a_deadline() {
    // C4 (fuel fuzz run 3): safepoints were only at jumps, calls and
    // returns, so 2,000 statements `b = s == t` over two 10 MB strings ran
    // to the end (7.5 s) with 100 units of fuel, and 20,000 of them (75 s)
    // past a 50 ms deadline. Each charged its work, but nothing checked:
    // now the compiler puts a `CHECK` in straight-line code, along every
    // path (a skipped branch, a handler, inside one expression).
    let engine = Engine::new(EngineConfig::default());
    // (The allocating ones use 1 MB: garbage is collected at safepoints.)
    let setup = "s = 'a' * 10_000_000\nt = 'a' * 9_999_999 + 'b'\nu = s[:1_000_000]\nl = list(range(20_000))\n";
    for op in [
        "b = s == t",
        "b = s < t",
        "b = 'b' in s",
        "b = u + 'x'",
        "b = u[1:]",
        "b = [*l]",
        "b = l == l[:]",
        // Two pieces: a lone field is its str, with no copy to check.
        "b = f'{u}x'",
        "if s == t:\n    b = 1",
        "try:\n    b = s == t\n    raise KeyError\nexcept KeyError:\n    pass",
        "b = (s == t and 0) or s < t",
    ] {
        let src = format!("{op}\n").repeat(2_000);
        let code = engine.compile(&src, "line.py").expect("compiles");
        for deadline in [false, true] {
            let out = Arc::new(Mutex::new(String::new()));
            let mut s = session(&engine, &out);
            let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
            assert!(matches!(p, Progress::Complete(_)), "{p:?}");
            let t = Instant::now();
            if deadline {
                s.set_deadline(Some(t + std::time::Duration::from_millis(20)));
            } else {
                s.set_fuel(Some(100));
            }
            let p = s.start(&code);
            let reason = if deadline {
                ParkReason::Deadline
            } else {
                ParkReason::Fuel
            };
            // 5df122e ran each to the end (`Complete`) before any check.
            assert_eq!(p, Progress::Parked { reason }, "{op}");
        }
    }
    // Granularity: each long operator after another stops for a safepoint
    // before it starts, so a deadline already past parks after every one
    // (a `CHECK` every 32 counted instructions let up to 63 run, 671 ms
    // over 60 MB strings [M]).
    let src = [
        "b = s == t",
        "b = s < t",
        "b = 'b' in s",
        "b = u + 'x'",
        "b = -big",
        "b = f'{l}'",
    ]
    .map(|op| format!("{op}\n").repeat(20))
    .concat();
    let code = engine.compile(&src, "each.py").expect("compiles");
    for deadline in [false, true] {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        let setup = format!("{setup}big = (1 << 2_807_355) // 3\n");
        let p = s.start(&engine.compile(&setup, "setup.py").expect("compiles"));
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        let arm = |s: &mut Session| {
            if deadline {
                s.set_deadline(Some(Instant::now()));
            } else {
                s.set_fuel(Some(1));
            }
        };
        let mut parks = 0;
        arm(&mut s);
        let mut p = s.start(&code);
        loop {
            match p {
                Progress::Parked { .. } => parks += 1,
                Progress::Complete(_) => break,
                p => panic!("{p:?}"),
            }
            arm(&mut s);
            p = s.resume(Resume::Continue).expect("resumes");
        }
        // 120 long statements: a park before every one but the first.
        assert!(parks >= 119, "parked {parks} times");
    }
    // One statement of 400 comparisons checks inside it too.
    let src = format!("b = ({})\n", "s == t, ".repeat(400));
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    s.set_fuel(Some(100));
    let p = s.start(&engine.compile(&src, "wide.py").expect("compiles"));
    assert_eq!(
        p,
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    );
}

#[test]
fn a_long_container_repr_parks_for_a_deadline_without_a_fuel_limit() {
    // Fuel-audit sweep: without a fuel limit `str()` of a container walked
    // it in one native step (558 ms for 1M small lists). The walk hands off
    // to the prelude twin after a fixed budget, so a deadline parks it long
    // before the whole text could be built.
    let engine = Engine::new(EngineConfig::default());
    let setup = "xs = [[i, (i,)] for i in range(600_000)]\n";
    let run = |deadline: bool| {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        let code = engine
            .compile("t = str(xs)\nprint(len(t), t[-12:])\n", "repr.py")
            .expect("compiles");
        let t = Instant::now();
        if deadline {
            s.set_deadline(Some(t + std::time::Duration::from_millis(1)));
        }
        let mut p = s.start(&code);
        let elapsed = t.elapsed();
        if deadline {
            assert_eq!(
                p,
                Progress::Parked {
                    reason: ParkReason::Deadline
                }
            );
            assert_eq!(out.lock().expect("output lock").as_str(), "");
            p = s.resume(Resume::Continue).expect("resumes");
        }
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        assert_eq!(
            out.lock().expect("output lock").as_str(),
            "12377780  (599999,)]]\n"
        );
        elapsed
    };
    let (whole, to_park) = (run(false), run(true));
    assert!(to_park * 3 < whole, "parked after {to_park:?} of {whole:?}");
}

#[test]
fn a_tuple_hash_over_shared_nesting_finishes() {
    // Fuel-audit finding: `hash` recursed into every item of a tuple, so 60
    // levels of `(t, t)` took 2^60 steps in one native call, ignoring fuel
    // and deadlines (CPython never finishes either). Past a fixed budget it
    // hands off to the prelude, which hashes each tuple once.
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let src = "\
def nest():
    t = ()
    for _ in range(60):
        t = (t, t)
    return t
t, u = nest(), nest()
print(hash(t) == hash(u), {t: 1}[t], t in {t})
";
    s.set_fuel(Some(20_000_000));
    let p = s.start(&engine.compile(src, "nest.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    // The native walk stops after a fixed number of tuples and the prelude,
    // charged per step, hashes each tuple once: 2.4M units [M]. A hash that
    // revisits shared tuples runs out of the 20M and parks.
    let fuel = 20_000_000 - s.fuel().expect("fuel");
    assert!(fuel < 6_000_000, "charged {fuel}");
    assert_eq!(out.lock().expect("output lock").as_str(), "True 1 True\n");
}

#[test]
fn guest_sized_padding_and_carets_raise_instead_of_aborting() {
    // Fuel-audit findings: a guest-chosen width reached an allocation
    // unchecked, so `'x'.center(10**15)` and the caret line of a reported
    // `SyntaxError` with a huge end offset aborted the process.
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let src = "\
for f in (str.center, str.ljust, str.rjust):
    try:
        f('x', 10**15)
    except MemoryError:
        print('MemoryError')
try:
    'x'.zfill(10**15)
except MemoryError:
    print('MemoryError')
";
    let p = s.start(&engine.compile(src, "pad.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "MemoryError\n".repeat(4)
    );
    let src = "raise SyntaxError('m', ('f', 1, 1, '', 1, 10**12))\n";
    let p = s.start(&engine.compile(src, "caret.py").expect("compiles"));
    let Progress::Error(e) = &p else {
        panic!("{p:?}")
    };
    assert!(e.traceback.ends_with("SyntaxError: m\n"), "{}", e.traceback);
}

#[test]
fn comparing_exponentially_shared_data_parks_on_fuel_and_on_a_deadline() {
    // 40 levels of `[x, 1, x]` (or frozensets of `x` and `(x, 1)`) have
    // 2^40 paths, which CPython walks too and never finishes. A native walk
    // hands off to the prelude after a fixed budget, so each comparison
    // parks instead of running to completion inside one native call.
    let setup = "\
def dag(leaf):
    x = leaf
    for _ in range(40):
        x = [x, 1, x]
    return x
def fdag():
    x = 'f'
    for _ in range(40):
        x = frozenset([x, (x, 1)])
    return x
x, y, z = dag('a'), dag('a'), dag('b')
fs, gs = fdag(), fdag()
c = [x]
c.append(c)
d = [y]
d.append(d)
";
    for op in [
        "x == y",
        "x <= y",
        "max([y, x])",
        "sorted([y, x])",
        "x in [z, y]",
        "fs == gs",
        "{fs: 1}[gs]",
        "c == d",
    ] {
        let engine = Engine::new(EngineConfig::default());
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        let code = engine
            .compile(&format!("{setup}{op}"), "<dag>")
            .expect("compiles");
        s.set_fuel(Some(100_000));
        let mut p = s.start(&code);
        for _ in 0..3 {
            assert_eq!(
                p,
                Progress::Parked {
                    reason: ParkReason::Fuel
                },
                "{op}"
            );
            s.set_fuel(Some(100_000));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        let snapshot = s.snapshot().expect("snapshot");
        s = engine.restore(&snapshot).expect("restore");
        s.set_fuel(None);
        s.set_deadline(Some(Instant::now() + std::time::Duration::from_millis(50)));
        let t = Instant::now();
        let p = s.resume(Resume::Continue).expect("resumes");
        assert_eq!(
            p,
            Progress::Parked {
                reason: ParkReason::Deadline
            },
            "{op}"
        );
        assert!(t.elapsed().as_secs() < 2, "{op}: took {:?}", t.elapsed());
    }
}

#[test]
fn a_catastrophic_regex_parks_on_fuel_and_on_a_deadline() {
    // `(a*)*b` against `a`s backtracks exponentially: the matcher must give
    // the fuel back in slices (and see the deadline) instead of hanging.
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let code = engine
        .compile("import re\nre.match(r'(a*)*b', 'a' * 64)", "<re>")
        .expect("compiles");
    s.set_fuel(Some(50_000));
    let mut p = s.start(&code);
    for _ in 0..5 {
        assert!(
            matches!(
                p,
                Progress::Parked {
                    reason: ParkReason::Fuel
                }
            ),
            "{p:?}"
        );
        s.set_fuel(Some(50_000));
        p = s.resume(Resume::Continue).expect("resumes");
    }
    s.set_fuel(None);
    s.set_deadline(Some(Instant::now() + std::time::Duration::from_millis(50)));
    let p = s.resume(Resume::Continue).expect("resumes");
    assert!(
        matches!(
            p,
            Progress::Parked {
                reason: ParkReason::Deadline
            }
        ),
        "{p:?}"
    );
    // A bounded one finishes the same parked or not.
    let src = "import re\nprint(re.findall(r'(\\w+)@(\\w+)', 'a@b ' * 3000)[-1], \
               re.sub(r'(x+x+)+y', '-', 'x' * 16 + 'z'))";
    let (plain, a) = run_plain(&[src], "<re>");
    let (parked, b, parks) = run_parked(&[src], "<re>", 300);
    assert_eq!((plain, a), (parked, b));
    assert!(parks > 10, "{parks} parks");
}

#[test]
fn an_int_operation_the_fuel_cannot_pay_parks_before_it_starts() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let code = engine
        .compile(
            "x = 3 ** 100_000\ny = x * x\nprint(y.bit_length())\n",
            "bigmul.py",
        )
        .expect("compiles");
    // `**` would charge 610,415 units (as `big_pow` does), more than the 10,000 set: the
    // run parks with the instruction not started.
    s.set_fuel(Some(10_000));
    let p = s.start(&code);
    assert_eq!(
        p,
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    );
    let pow = s.fuel_wanted();
    assert_eq!(pow, 610_415);
    // The park survives a snapshot: a restored session reports the same.
    let bytes = s.snapshot().expect("snapshot");
    let mut s = engine.restore(&bytes).expect("restores");
    let sink = Arc::clone(&out);
    s.set_output(Box::new(move |t: &str| {
        sink.lock().expect("output lock").push_str(t);
    }));
    assert_eq!(s.fuel_wanted(), pow);
    // Resuming runs it (charging the excess); the `*` after it, about
    // 3,000,000 units, parks the same way, before `print`.
    s.set_fuel(Some(10_000));
    let p = s.resume(Resume::Continue).expect("resumes");
    assert_eq!(
        p,
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    );
    let mul = s.fuel_wanted();
    assert!(mul > 2_000_000, "{mul}");
    assert_eq!(out.lock().expect("output lock").as_str(), "");
    // An ordinary park reports nothing wanted; enough fuel runs it all
    // (with room for building the 40 KB result and `bit_length`, one unit
    // per 64 bytes each).
    s.set_fuel(Some(mul + 10_000));
    let p = s.resume(Resume::Continue).expect("resumes");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(s.fuel_wanted(), 0);
    assert_eq!(out.lock().expect("output lock").as_str(), "316993\n");
}

#[test]
fn powers_of_zero_and_one_never_park_for_fuel() {
    // `big_pow` answers 0, 1 and -1 at once; their cost must not be
    // estimated as a full multiplication chain.
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let code = engine
        .compile(
            "print(1 ** 1_000_000_000, (-1) ** 1_000_000_001, 0 ** 1_000_000_000)\n",
            "pow01.py",
        )
        .expect("compiles");
    s.set_fuel(Some(10_000));
    let p = s.start(&code);
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(s.fuel_wanted(), 0);
    assert_eq!(out.lock().expect("output lock").as_str(), "1 -1 0\n");
}

#[test]
fn big_int_builtins_the_fuel_cannot_pay_park_before_they_start() {
    // Three-argument `pow`, `pow`, `divmod`, `sum`, `math.prod` and the math
    // functions over big ints park as the operators do. The operands are built in a cell
    // without a fuel limit, so only the builtin can park. A builtin called
    // by a native (`map`) runs, charging after. What a builtin asks for is
    // its charge, within a factor of two of what the cell spends unparked.
    let engine = Engine::new(EngineConfig::default());
    // Each under the 50 ms one operation may take (`intops::WORK_MAX`).
    let setup = "import math\nm = 3 ** 1_500 + 2\nx = 3 ** 40_000\ny = x * x\nz = 3 ** 20_000\nw = 3 ** 10_000\n";
    for (src, want) in [
        ("print(pow(m - 2, m, m) % 1000)", "717"),
        ("print(pow(x, 3).bit_length())", "190196"),
        ("print(pow(base=m - 2, exp=m, mod=m) % 1000)", "717"),
        (
            "q, r = divmod(y, x + 1)\nprint(q.bit_length(), r.bit_length())",
            "63399 1",
        ),
        ("print(math.factorial(30_000).bit_length())", "402909"),
        ("print(math.comb(40_000, 20_000).bit_length())", "39993"),
        ("print(math.isqrt(z).bit_length())", "15850"),
        ("print(math.gcd(z, w * 2).bit_length())", "15850"),
        ("print(sum([x] * 3_000).bit_length())", "63411"),
        ("print(math.prod([x, x, x]).bit_length())", "190196"),
        (
            "print([r % 1000 for r in map(pow, [m - 2], [m], [m])])",
            "[717]",
        ),
    ] {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        s.set_fuel(Some(20_000));
        let mut p = s.start(&engine.compile(src, "big.py").expect("compiles"));
        let mut wanted = 0;
        while p
            == (Progress::Parked {
                reason: ParkReason::Fuel,
            })
        {
            wanted = wanted.max(s.fuel_wanted());
            s.set_fuel(Some(20_000));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
        // The same cell, unparked.
        s.set_fuel(Some(u64::MAX / 2));
        let p = s.start(&engine.compile(src, "big.py").expect("compiles"));
        assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
        let spent = u64::MAX / 2 - s.fuel().expect("fuel");
        if src.contains("map(") {
            assert_eq!(wanted, 0, "{src}");
        } else {
            assert!(wanted >= 1 << 16, "{src}: wanted {wanted}");
            assert!(
                (spent / 2..=spent * 2).contains(&wanted),
                "{src}: wanted {wanted}, spent {spent}"
            );
        }
        let o = out.lock().expect("output lock");
        let mut lines = o.lines();
        assert_eq!(lines.next(), Some(want), "{src}");
        assert_eq!(lines.next(), Some(want), "{src}");
    }
}

#[test]
fn a_long_sort_and_a_big_factorial_ask_for_their_real_cost_before_starting() {
    // Fuel-audit findings: sorting 4M ints is one 2 s native step, and
    // `math.factorial(400_000)` one of 2.4 s that charged 237,800 units
    // (about 10 ns each). Both park before they start when the fuel left
    // cannot pay their estimate, which `fuel_wanted` reports; a factorial
    // that size now fails at once (`intops::WORK_MAX`, steps.rs), so the
    // rows use ones the cap accepts.
    let engine = Engine::new(EngineConfig::default());
    let setup = "import math\nxs = list(range(1_000_000, 0, -1))\nt = tuple(xs)\n";
    for (src, at_least) in [
        ("xs.sort()", 20_000_000),
        ("ys = sorted(xs)", 20_000_000),
        ("ys = sorted(t, reverse=True)", 20_000_000),
        ("ys = sorted(xs, key=abs)", 20_000_000),
        ("y = math.factorial(40_000)", 2_000_000),
        ("y = math.perm(40_000)", 2_000_000),
        ("y = math.perm(80_000, 40_000)", 2_000_000),
        ("ys = sorted(xs[:1000])", 0),
    ] {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        s.set_fuel(Some(20_000));
        let p = s.start(&engine.compile(src, "long.py").expect("compiles"));
        if at_least == 0 {
            assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
            continue;
        }
        assert_eq!(
            p,
            Progress::Parked {
                reason: ParkReason::Fuel
            },
            "{src}"
        );
        let wanted = s.fuel_wanted();
        assert!(wanted >= at_least, "{src}: wanted {wanted}");
        assert!(wanted < at_least * 8, "{src}: wanted {wanted}");
    }
}

#[test]
fn a_long_sort_parks_between_slices_for_a_deadline() {
    // Sorting 1M strs was one native step of 1.7 s [M]: a deadline or park
    // request waited for all of it, and under mm-wasm the epoch grace
    // trapped. A long sort now merges a slice per step (and a keyed one
    // checks every 4,096 keys): with no fuel limit and a deadline already
    // past, it parks once per slice, and resumed ends sorted.
    let engine = Engine::new(EngineConfig::default());
    let setup = "xs = [str((i * 7919) % 300_007) for i in range(300_000)]\nt = tuple(xs)\n";
    for (src, check) in [
        ("ys = sorted(xs)", "ys == sorted(t)"),
        ("xs.sort(reverse=True)", "xs == sorted(t, reverse=True)"),
        ("ys = sorted(t, key=len)", "ys == sorted(t, key=len)"),
    ] {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        let code = format!("{src}\nprint({check})\n");
        let mut parks = 0;
        s.set_deadline(Some(Instant::now()));
        let mut p = s.start(&engine.compile(&code, "sort.py").expect("compiles"));
        while p
            == (Progress::Parked {
                reason: ParkReason::Deadline,
            })
        {
            parks += 1;
            // Past again for the sort; the check then runs unlimited.
            let more = parks < 1000;
            s.set_deadline(more.then(Instant::now));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
        // 300,000 items: about 40 slices of the merge (5df122e: 2 parks).
        assert!(parks >= 20, "{src}: parked {parks} times");
        assert_eq!(out.lock().expect("output lock").as_str(), "True\n", "{src}");
    }
}

#[test]
fn an_operator_after_an_ordinary_fuel_park_still_parks_before_starting() {
    // The first `*` parks before starting and, resumed, overdraws the
    // slice; the call to `f` then parks for fuel as usual. Resuming that
    // park must not let the `*` inside `f` run unasked.
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let setup = "x = 3 ** 40_000\ndef f(a, b):\n    return a * b\n";
    let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let src = "y = x * x\nz = f(x, x)\nprint(y == z)\n";
    s.set_fuel(Some(20_000));
    let mut p = s.start(&engine.compile(src, "big.py").expect("compiles"));
    let mut wanted = Vec::new();
    while p
        == (Progress::Parked {
            reason: ParkReason::Fuel,
        })
    {
        wanted.push(s.fuel_wanted());
        s.set_fuel(Some(20_000));
        p = s.resume(Resume::Continue).expect("resumes");
    }
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(out.lock().expect("output lock").as_str(), "True\n");
    let big: Vec<u64> = wanted.iter().copied().filter(|&w| w != 0).collect();
    assert_eq!(big.len(), 2, "both multiplications park first: {wanted:?}");
}

#[test]
fn a_park_request_survives_start_and_cancel_until_a_cell_parks() {
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).expect("session");
    let handle = s.park_handle();
    let looping = engine
        .compile("t = 0\nfor i in range(1000):\n    t += i\nt", "loop.py")
        .expect("compiles");
    // Requested between cells: the next cell parks at its first safepoint.
    handle.request();
    assert_eq!(
        s.start(&looping),
        Progress::Parked {
            reason: ParkReason::Requested
        }
    );
    // Taken by that park: the cell then runs to its end.
    assert!(matches!(
        s.resume(Resume::Continue),
        Ok(Progress::Complete(_))
    ));
    // Requested while a cell is stopped and the cell cancelled: the
    // request stays for the next cell.
    s.set_fuel(Some(10));
    assert!(matches!(s.start(&looping), Progress::Parked { .. }));
    handle.request();
    assert!(s.cancel());
    assert_eq!(
        s.start(&looping),
        Progress::Parked {
            reason: ParkReason::Requested
        }
    );
}

#[test]
fn fuel_is_per_cell_and_the_session_total_caps_every_cell() {
    let engine = Engine::new(
        EngineConfig::new().with_limits(micro_monty::Limits::new().with_fuel(Some(500))),
    );
    let mut s = Session::new(&engine).expect("session");
    let short = engine.compile("x = 1", "short.py").expect("compiles");
    let long = engine
        .compile("t = 0\nfor i in range(2000):\n    t += i", "long.py")
        .expect("compiles");
    // Each cell starts with `Limits::fuel`, whatever the last one left.
    assert!(matches!(
        s.start(&long),
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    ));
    assert_eq!(s.fuel(), Some(0));
    assert!(s.cancel());
    assert!(matches!(s.start(&short), Progress::Complete(_)));
    assert!(s.fuel().is_some_and(|f| f > 400), "{:?}", s.fuel());
    // `set_fuel` between cells gives the next cell that budget, once.
    s.set_fuel(Some(5000));
    assert!(matches!(s.start(&long), Progress::Complete(_)));
    let spent = 5000 - s.fuel().expect("fuel");
    assert!(spent > 500, "{spent}");
    assert!(matches!(s.start(&long), Progress::Parked { .. }));
    assert!(s.cancel());

    // A total: each cell gets the smaller of its budget and what is left,
    // and what it charges, refuelled or not, comes off the total.
    assert!(s.environment().contains("500 fuel units per cell."));
    s.set_fuel_total(Some(700));
    assert_eq!(s.fuel_total(), Some(700));
    assert!(
        s.environment()
            .contains("500 fuel units per cell and 700 left for the session."),
        "{}",
        s.environment()
    );
    assert!(matches!(
        s.start(&long),
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    ));
    assert_eq!(s.fuel_total(), Some(200));
    // A snapshot keeps what is left.
    let restored = engine
        .restore(&s.snapshot().expect("snapshot"))
        .expect("restore");
    assert_eq!(restored.fuel_total(), Some(200));
    s.set_fuel(Some(150));
    assert!(matches!(
        s.resume(Resume::Continue),
        Ok(Progress::Parked { .. })
    ));
    assert_eq!(s.fuel_total(), Some(50));
    assert!(s.cancel());
    assert!(matches!(
        s.start(&long),
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    ));
    assert_eq!(s.fuel_total(), Some(0));
    assert!(s.cancel());
    // Spent: every cell parks at its first safepoint, for good.
    assert!(matches!(
        s.start(&long),
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    ));
    assert!(s.cancel());
    s.set_fuel_total(None);
    assert!(matches!(s.start(&short), Progress::Complete(_)));
}

#[test]
fn fuel_is_one_unit_per_call_and_backward_jump_however_the_cell_parks() {
    // Fuel is charged exactly per safepoint, a call or a backward jump
    // (docs/spec.md §5), plus one for the cell's own frame, whether the cell
    // runs unparked or parks every few units.
    let engine = Engine::new(EngineConfig::default());
    let n = 3_000;
    for (src, want) in [
        (format!("for k in range({n}):\n    pass\n"), n + 1),
        (
            format!("def f(k):\n    return k\nfor k in range({n}):\n    f(k)\n"),
            2 * n + 1,
        ),
        (
            format!(
                "def g(k):\n    return k\ndef f(k):\n    return g(k)\nfor k in range({n}):\n    f(k)\n"
            ),
            3 * n + 1,
        ),
    ] {
        for slice in [None, Some(1), Some(7), Some(1_000)] {
            let out = Arc::new(Mutex::new(String::new()));
            let mut s = session(&engine, &out);
            s.set_fuel_total(Some(u64::MAX / 2));
            s.set_fuel(Some(slice.unwrap_or(1 << 40)));
            let mut p = s.start(&engine.compile(&src, "fuel.py").expect("compiles"));
            while let Progress::Parked { .. } = p {
                s.set_fuel(slice);
                p = s.resume(Resume::Continue).expect("resumes");
            }
            assert!(matches!(p, Progress::Complete(_)), "{p:?}");
            let spent = u64::MAX / 2 - s.fuel_total().expect("total");
            assert_eq!(spent, want, "{src} in slices of {slice:?}");
        }
    }
}

#[test]
fn a_cell_spends_the_same_fuel_however_it_parks() {
    // The total a cell charges (`Session::fuel_total`, which counts every
    // stretch's fuel and overdraft) does not depend on where it parks:
    // at safepoints mid-period, before int operations it cannot pay,
    // around collections that big results request, inside calls and
    // generators, and in `re`, whose slices end at a step budget that
    // does not follow the fuel left. Each slice size parks at different
    // points.
    let engine = Engine::new(EngineConfig::default());
    let setup = "import re\nx = 3 ** 40_000\n";
    let src = "\
def f(i):
    return [i] * 3
def g(n):
    for i in range(n):
        yield f(i)
t = 0
for k in range(40):
    s = 'ab' * 70_000
    t += len(s) + sum(len(v) for v in g(20))
    if k % 10 == 0:
        t += (x * x).bit_length() + (x ** 2 % 7)
        t += len(re.findall(r'b(a)', s)) + len(re.split(r'(?<=a)b', s[:9_000]))
print(t)
";
    let total = u64::MAX / 2;
    let spent = |slice: Option<u64>| {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&engine, &out);
        let p = s.start(&engine.compile(setup, "setup.py").expect("compiles"));
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        s.set_fuel_total(Some(total));
        s.set_fuel(Some(slice.unwrap_or(1 << 40)));
        let mut p = s.start(&engine.compile(src, "fuel.py").expect("compiles"));
        let mut parks = 0;
        while let Progress::Parked { .. } = p {
            parks += 1;
            s.set_fuel(slice);
            p = s.resume(Resume::Continue).expect("resumes");
        }
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        assert_eq!(out.lock().expect("output lock").as_str(), "6407600\n");
        (total - s.fuel_total().expect("total"), parks)
    };
    let (want, _) = spent(None);
    for slice in [1, 13, 997, 30_000] {
        let (got, parks) = spent(Some(slice));
        assert!(parks > 0, "slice {slice}");
        assert_eq!(got, want, "slice {slice}, {parks} parks");
    }
}
