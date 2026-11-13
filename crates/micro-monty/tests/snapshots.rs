//! FULL snapshots (docs/spec.md §9.3): a session saved at any stop (a park
//! at any depth, a host call, a wait on host futures, between cells) and
//! restored continues exactly as the original would, with the same call
//! ids; a restored session saves to the same bytes as the original;
//! malformed snapshots are typed errors, never panics.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    Engine, EngineConfig, HostModule, HostValue, Limits, Progress, Resume, Session,
    SessionSettings, SnapshotError, Stream, TextCut,
};
use std::{
    fmt::Write as _,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

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

fn engine() -> Engine {
    Engine::new(EngineConfig::default())
}

fn attach(session: &mut Session, out: &Arc<Mutex<String>>) {
    let sink = Arc::clone(out);
    session.set_output_streams(Box::new(move |stream, s: &str| {
        let mut o = sink.lock().expect("output lock");
        if stream == Stream::Stderr {
            o.push_str("[err]");
        }
        o.push_str(s);
    }));
}

fn outcome(p: &Progress) -> String {
    match p {
        Progress::Complete(c) => format!("complete {:?}", c.as_ref().map(|c| &c.repr)),
        Progress::Error(e) => format!("error\n{}", e.traceback),
        other => panic!("unexpected {other:?}"),
    }
}

fn plain(src: &str, name: &str) -> (String, String) {
    let e = engine();
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = Session::new(&e).expect("session");
    attach(&mut s, &out);
    let p = s.start(&e.compile(src, name).expect("compiles"));
    let text = out.lock().expect("output lock").clone();
    (text, outcome(&p))
}

/// Parks every `fuel` units; at every `every`-th park, snapshots, restores
/// into a new session (dropping the old one) and continues there. Every
/// second hop also checks that the restored session saves to the bytes the
/// original saves to next.
#[test]
fn a_restore_inside_each_kind_of_suspended_call_resumes_it() {
    // Parks at every safepoint, restoring each time, inside star-unpacking
    // of a user iterator, `yield from`, a `getattr` default over a user
    // `__getattr__` and an exception raised in a user `__eq__` under a
    // list comparison, `raise C` running C's `__init__`, and a property
    // getter's AttributeError falling back to `__getattr__`: each leaves its
    // own continuation on the stack.
    let src = "\
class It:
    def __init__(self):
        self.i = 0
    def __iter__(self):
        return self
    def __next__(self):
        self.i += 1
        if self.i > 3:
            raise StopIteration
        return self.i
a, *b = It()
print(a, b)
def g():
    yield from range(2)
    yield from It()
print(list(g()))
class A:
    def __getattr__(self, name):
        if name == 'x':
            return 5
        raise AttributeError(name)
print(getattr(A(), 'x', 1), getattr(A(), 'y', 2))
class E:
    def __eq__(self, o):
        raise ValueError('eq')
try:
    [E()] == [E()]
except ValueError as e:
    print('raised', e)
class Made(Exception):
    def __init__(self):
        super().__init__(sum(range(4)))
try:
    raise Made
except Made as e:
    print('made', e)
class P:
    @property
    def x(self):
        raise AttributeError('x')
    def __getattr__(self, name):
        return name * 2
print(P().x)
";
    let (want, want_outcome) = plain(src, "conts.py");
    assert_eq!(
        want,
        "1 [2, 3]\n[0, 1, 1, 2, 3]\n5 2\nraised eq\nmade 6\nxx\n"
    );
    let (got, got_outcome, parks) = hopping(src, "conts.py", 1, 1);
    assert!(parks > 50, "{parks} parks");
    assert_eq!((got, got_outcome), (want, want_outcome));
}

#[test]
fn a_restore_inside_protocol_and_generator_calls_resumes_them() {
    // As above, inside a user `__contains__` under `in` and `not in`, a
    // user `__exit__` with and without an exception, a generator's `send`,
    // `throw` delegated through `yield from` and `close` running its
    // `finally`, an `await` of a user awaitable, and a keyed sort.
    let src = "\
class C:
    def __contains__(self, x):
        return x == sum(range(3))
print(3 in C(), 4 not in C())
class W:
    def __enter__(self):
        return self
    def __exit__(self, t, v, tb):
        print('exit', t.__name__ if t else None, sum(range(3)))
        return True
with W():
    raise KeyError('k')
with W():
    pass
def h():
    try:
        x = yield 1
        while True:
            try:
                x = yield x * 2
            except ValueError:
                x = sum(range(6))
    finally:
        print('closing', sum(range(5)))
gen = h()
next(gen)
print(gen.send(4), gen.send(5))
gen.close()
def d():
    r = yield from h()
gd = d()
next(gd)
print(gd.send(7), gd.throw(ValueError))
gd.close()
import asyncio
class Aw:
    def __await__(self):
        if False:
            yield
        return sum(range(4))
async def m():
    return await Aw()
print(asyncio.run(m()))
print(sorted(range(5), key=lambda v: -v))
";
    let (want, want_outcome) = plain(src, "conts2.py");
    assert_eq!(
        want,
        "True True\nexit KeyError 3\nexit None 3\n8 10\nclosing 10\n14 30\nclosing 10\n6\n[4, 3, 2, 1, 0]\n"
    );
    let (got, got_outcome, parks) = hopping(src, "conts2.py", 1, 1);
    assert!(parks > 50, "{parks} parks");
    assert_eq!((got, got_outcome), (want, want_outcome));
}

fn hopping(src: &str, name: &str, fuel: u64, every: usize) -> (String, String, usize) {
    let e = engine();
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = Session::new(&e).expect("session");
    attach(&mut s, &out);
    s.set_fuel(Some(fuel));
    let mut p = s.start(&e.compile(src, name).expect("compiles"));
    let mut parks = 0;
    while let Progress::Parked { .. } = p {
        parks += 1;
        if parks % every != 0 {
            s.set_fuel(Some(fuel));
            p = s.resume(Resume::Continue).expect("resumes");
            continue;
        }
        let bytes = s.snapshot().expect("snapshot");
        let mut r = e.restore(&bytes).expect("restore");
        if parks % (2 * every) == 0 {
            let again = s.snapshot().expect("snapshot");
            assert_eq!(
                r.snapshot().expect("snapshot"),
                again,
                "{name}: park {parks}"
            );
        }
        attach(&mut r, &out);
        r.set_fuel(Some(fuel));
        s = r;
        p = s.resume(Resume::Continue).expect("resumes");
    }
    let text = out.lock().expect("output lock").clone();
    (text, outcome(&p), parks)
}

/// Whether an operation that ran out of memory runs again after a
/// collection must not depend on the host's snapshots: the retry needs
/// 1 MiB allocated since the last collection (`Vm::oom_retry_if`), and a
/// snapshot's collection once zeroed that count, so `t = None` then left
/// garbage with nothing allocated and the call raised `MemoryError` only
/// when the cell was snapshotted and restored at its parks (the 4th-pass
/// review [M]). Unparked, sliced, and sliced with a restore at every park
/// must print the same, as python3.14 does.
#[test]
fn a_retry_after_memory_error_does_not_depend_on_snapshots() {
    for (pad, op) in [
        (31_150_000, "s = a.center((4 << 20) + 1)"),
        (31_437_184, "s = a + 'b'"),
    ] {
        let src = format!(
            "import gc\nkeep = [bytearray(1 << 20) for _ in range(216)]\npad = bytearray({pad})\n\
             a = 'a' * (4 << 20)\ngc.collect()\nt = [0] * 300000\nt = None\n\
             try:\n    {op}\n    print('ok', len(s))\nexcept MemoryError:\n    print('ME')\n"
        );
        let (want, _) = plain(&src, "retry.py");
        assert_eq!(want, "ok 4194305\n", "{op}: unparked");
        let (sliced, _, parks) = hopping(&src, "retry.py", 99_991, usize::MAX);
        assert_eq!(sliced, want, "{op}: sliced");
        assert!(parks > 1, "{op}: {parks} parks");
        let (restored, _, _) = hopping(&src, "retry.py", 99_991, 1);
        assert_eq!(restored, want, "{op}: restored at every park");
    }
}

/// Nor on where the host's fuel slices end: a builtin that drew a one-shot
/// iterator marked the instruction not to run again (`oom_rerun`) and left
/// its items in `consumed`, and only the next safepoint's check dropped
/// them, so a later instruction's `MemoryError` was retried only when a
/// slice had ended in between (`ok` at slices of 1 to 3, `ME` unparked and
/// at 7 to 100,000; the 4th-pass review [M]). Both marks now belong to the
/// instruction that set them (`Vm::op_stamp`). The second heap holds a
/// 2,000,000-item list, so the retry's collection takes 9 steps, not 2,
/// and the slices end inside its wait [M].
#[test]
fn a_retry_after_memory_error_does_not_depend_on_fuel_slices() {
    for (heap, (gather, op, want)) in [
        "keep = [bytearray(1 << 20) for _ in range(236)]",
        "keep = [bytearray(1 << 20) for _ in range(220)]\nobjs = list(range(2000000))",
    ]
    .into_iter()
    .flat_map(|heap| {
        [
            (
                "x = list(g); sorted([1], key=None)",
                "s = b'a' * 3000000",
                "ok 3000000\n",
            ),
            ("x = list(g)", "s = b'a'.ljust(3000000)", "ok 3000000\n"),
            // A slice store that drew the iterator, with no call or backward
            // jump before the operator: its mark still holds there.
            ("x = [0]\nx[:] = g", "s = b'a' * 3000000", "ME\n"),
        ]
        .map(|case| (heap, case))
    }) {
        let src = format!(
            "import gc\n{heap}\npad = bytearray(16500000)\n\
             g = iter([1, 2, 3])\ngc.collect()\nt = [0] * 300000\nt = None\n{gather}\n\
             try:\n    {op}\n    print('ok', len(s))\nexcept MemoryError:\n    print('ME')\n"
        );
        let (unparked, _) = plain(&src, "slices.py");
        assert_eq!(unparked, want, "{heap}; {gather}: unparked");
        for slice in [1, 2, 3, 7, 100, 1_000, 100_000] {
            let (sliced, _, _) = hopping(&src, "slices.py", slice, usize::MAX);
            assert_eq!(sliced, want, "{heap}; {op}: slices of {slice}");
        }
        let (restored, _, parks) = hopping(&src, "slices.py", 99_991, 1);
        assert!(parks > 1, "{heap}; {op}: {parks} parks");
        assert_eq!(restored, want, "{heap}; {op}: restored at every park");
    }
}

/// A snapshot's collection frees what the bytes allocated before it left,
/// so the collection trigger must not count them again after it: with
/// `set_gc_yield` (a safepoint that ran a collection step parks with fuel
/// left) and a snapshot at every park, keeping the count made the first
/// safepoint after each restore collect and park again, and every slice
/// ended after about 95 units (the 4th-pass review: past park 76 of this
/// cell, about 35,000 parks [E], against 62 without snapshots [M]). The
/// retry rule still counts them (`Heap::allocated_since_own_gc`).
#[test]
fn a_snapshot_at_every_park_does_not_shorten_yielding_slices() {
    let src = "l = []\nfor i in range(120000):\n    l.append('x' * 1000 + str(i))\nprint(sum(len(s) for s in l))\n";
    let parks = |restore: bool| {
        let e = engine();
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = Session::new(&e).expect("session");
        attach(&mut s, &out);
        s.set_gc_yield(true);
        s.set_fuel(Some(50_000));
        let mut p = s.start(&e.compile(src, "yield.py").expect("compiles"));
        let mut parks = 0;
        while let Progress::Parked { .. } = p {
            parks += 1;
            assert!(parks < 1_000, "restore {restore}: {parks} parks");
            if restore {
                let bytes = s.snapshot().expect("snapshot");
                let mut r = e.restore(&bytes).expect("restore");
                attach(&mut r, &out);
                r.set_gc_yield(true);
                s = r;
            }
            s.set_fuel(Some(50_000));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        assert_eq!(*out.lock().expect("output lock"), "120608890\n");
        parks
    };
    let (plain, restored) = (parks(false), parks(true));
    assert!(
        restored <= plain + plain / 4 + 2,
        "{restored} parks restored at every park, {plain} without"
    );
}

/// Nor on a collection between cells: `Session::compile` collects when the
/// last cell's garbage is in the way of compiling the next, and that once
/// reset the count, so a retry in the next cell did not run where it did
/// after a snapshot between the cells, or with `Engine::compile` (`ME`
/// against `ok` at strs of 58.0M to 58.45M chars at 256 MiB; the 4th-pass
/// review's G-c [M]). The 100,000-byte comment is what compiling the cell
/// needs room for.
#[test]
fn a_retry_after_memory_error_does_not_depend_on_collections_between_cells() {
    let cell1 = "keep = [bytearray(1 << 20) for _ in range(200)]\nt = bytearray(30 << 20)\nx = 0\n\
                 for i in range(3000): x += i\nt = None\nu = bytearray(3 << 20)\nu = None\n";
    let pad = format!("#{}\n", "x".repeat(100_000));
    for n in [58_000_000, 58_200_000, 58_400_000] {
        let cell2 = format!(
            "t = [0] * 60000\nt = None\ntry:\n    s = 'a' * {n}\n    print('ok')\n\
             except MemoryError:\n    print('ME')\n{pad}"
        );
        let mut seen = Vec::new();
        for mode in ["engine", "session", "restore", "snapshot"] {
            let e =
                Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(256 << 20)));
            let out = Arc::new(Mutex::new(String::new()));
            let mut s = Session::new(&e).expect("session");
            attach(&mut s, &out);
            for (i, src) in [cell1, cell2.as_str()].into_iter().enumerate() {
                if i > 0 && mode == "restore" {
                    let bytes = s.snapshot().expect("snapshot");
                    s = e.restore(&bytes).expect("restore");
                    attach(&mut s, &out);
                } else if i > 0 && mode == "snapshot" {
                    s.snapshot().expect("snapshot");
                }
                let code = if mode == "engine" {
                    e.compile(src, "cell.py")
                } else {
                    s.compile(src, "cell.py")
                };
                let p = s.start(&code.expect("compiles"));
                assert!(matches!(p, Progress::Complete(_)), "{n} {mode}: {p:?}");
            }
            seen.push((mode, out.lock().expect("output lock").clone()));
        }
        assert!(seen.iter().all(|(_, o)| *o == seen[0].1), "{n}: {seen:?}");
    }
}

/// `gc.collect()` waits for its collection, whose steps depend on parks,
/// snapshots (a snapshot's collection finishes it at once), gc-yield and
/// the target. Its guest loop (`while _mm.gc_wait(): pass`) charged 2 units
/// a step, so the cell's fuel depended on all of them (the reviewer's F1
/// [M]). A snapshot while it waited for a mark that began before the call,
/// with the session then going on, left it waiting for a collection nothing
/// started: it spun, charging fuel, until the fuel or a deadline ran out
/// (4b32aa2 too [M]). Every run here charges the unparked run's fuel and
/// prints its output: sliced; sliced with a restore or a snapshot at every
/// park, or with gc-yield; and parked by the host after each `print`, so
/// inside the wait, with a snapshot at every park, or at one, each in turn.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one program, its runs and their modes"
)]
fn gc_collect_charges_the_same_fuel_however_the_run_is_parked() {
    #[derive(Clone, Copy, Debug)]
    enum Snap {
        No,
        Restore,
        Keep,
        Once(usize),
    }
    let src = "\
import gc
pad = bytearray(32 << 20)
keep = list(map(list, zip(range(400000))))
gc.collect()
for n in range(540, 610, 5):
    for i in range(n):
        t = bytes(4000)
    t = None
    print('@park', n)
    gc.collect()
print('done')
";
    // Parks every `slice` units, or (`None`) where the host asks after each
    // `print`; `most` bounds the fuel, so a cell that spins fails at once.
    let run = |slice: Option<u64>, snap: Snap, gc_yield: bool, most: u64| {
        let e = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(64 << 20)));
        let out = Arc::new(Mutex::new(String::new()));
        let hook = |s: &mut Session| {
            let sink = Arc::clone(&out);
            let park = s.park_handle();
            let request = slice.is_none();
            s.set_output(Box::new(move |t: &str| {
                if request && t.contains("@park") {
                    park.request();
                }
                sink.lock().expect("output lock").push_str(t);
            }));
            s.set_gc_yield(gc_yield);
        };
        let total = u64::MAX / 2;
        let budget = slice.unwrap_or(5_000_000);
        let mut s = Session::new(&e).expect("session");
        hook(&mut s);
        s.set_fuel_total(Some(total));
        s.set_fuel(Some(budget));
        let mut p = s.start(&e.compile(src, "collect.py").expect("compiles"));
        let mut parks = 0;
        while let Progress::Parked { .. } = p {
            parks += 1;
            let spent = total - s.fuel_total().expect("a total");
            assert!(
                spent <= most,
                "{slice:?} {snap:?} {gc_yield}: {spent} units by park {parks}"
            );
            match snap {
                Snap::Restore => {
                    let bytes = s.snapshot().expect("snapshot");
                    s = e.restore(&bytes).expect("restore");
                    hook(&mut s);
                }
                Snap::Keep => {
                    s.snapshot().expect("snapshot");
                }
                Snap::Once(k) if k == parks => {
                    s.snapshot().expect("snapshot");
                }
                _ => {}
            }
            s.set_fuel(Some(budget));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        assert!(
            matches!(p, Progress::Complete(_)),
            "{slice:?} {snap:?}: {p:?}"
        );
        let text = out.lock().expect("output lock").clone();
        (text, total - s.fuel_total().expect("a total"), parks)
    };
    let (want, fuel, _) = run(Some(u64::MAX / 4), Snap::No, false, u64::MAX);
    assert!(want.ends_with("@park 605\ndone\n"), "{want}");
    // A mark is running at the calls of n = 565 to 580 (parks 6 to 9) [M].
    let mut runs = vec![
        (Some(300_007), Snap::No, false),
        (Some(300_007), Snap::Restore, false),
        (Some(300_007), Snap::Keep, false),
        (Some(300_007), Snap::No, true),
        (Some(300_007), Snap::Restore, true),
        (None, Snap::Keep, false),
        (None, Snap::Keep, true),
    ];
    runs.extend((4..=12).map(|k| (None, Snap::Once(k), false)));
    std::thread::scope(|scope| {
        let runs: Vec<_> = runs
            .into_iter()
            .map(|(slice, snap, gc_yield)| {
                let run = &run;
                let t = scope.spawn(move || run(slice, snap, gc_yield, fuel + fuel / 2));
                ((slice, snap, gc_yield), t)
            })
            .collect();
        for (how, t) in runs {
            let (got, charged, parks) = t.join().expect("run");
            assert_eq!(got, want, "{how:?}");
            assert_eq!(charged, fuel, "{how:?}: {parks} parks");
        }
    });
}

/// A snapshot while an instruction that ran out of memory waits for its
/// retry's collection (`Vm::oom_retry_if`) is that collection: the restored
/// instruction is the rerun. The snapshot once kept the count the retry
/// needs and the restore dropped the wait, so the rerun asked for a new
/// collection, which parked again before it ended, at every park: with a
/// restore at every park the cell never ended (the reviewer's H1 [M]).
/// In the first two shapes the heap holds a 900,000-item list, so the
/// collection takes several steps, and the operation charges more than a
/// slice, so the wait parks at once; the third needs the restored rerun
/// mark. Every run must print the unparked run's output and charge its
/// fuel.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "three programs, their runs and their modes"
)]
fn a_restore_inside_a_retry_wait_reruns_the_instruction_once() {
    let head = "\
import gc
c = {'i': 0, 'li': 0}
class Idx:
    def __index__(self): c['i'] += 1; return 2
class L(list): pass
class T(tuple): pass
class TI(tuple):
    def __iter__(self): c['li'] += 1; return iter(src)
src = list(range(200000))
keep = [bytearray(1 << 20) for _ in range(236)]
";
    let tail = "\
gc.collect()
t = [0] * 300000
t = None
try:
    OP
    st = 'ok'
except MemoryError:
    st = 'ME'
print(st, type(x).__name__, x is y, list.__len__(x), c)
";
    // `f(*big, **kw)` (`CALL_EX`) builds its 1.6 MiB `*args` tuple before
    // the copy of `kw` fails, so the rerun allocates over 1 MiB before it
    // fails again: a rerun not marked as one retried again, after every
    // restore (the reviewer's guard check [M]), and at slices of 50,003
    // after every park, with no snapshot [M]. A snapshot just before the
    // call frees the cell's garbage first, so the first attempt gets
    // further before it fails and charges 28,672 units more [M]: its fuel
    // is not compared (a snapshot collects first).
    let callex = "\
import gc
def f(*a, **k): return len(a) + len(k)
big = list(range(200000))
kw = {'k%d' % i: i for i in range(120000)}
s = ','.join(['x' * 1000] * 3000)
keep = [bytearray(1 << 20) for _ in range(200)]
pad = bytearray(28000000)
gc.collect()
t = [0] * 300000
t = None
try:
    r = f(*big, **kw)
    st = 'ok'
except MemoryError:
    st = 'ME'
print(st)
";
    for (src, op, want, same_fuel) in [
        (
            format!(
                "{head}pad = bytearray(6000000)\nx = list(range(900000))\ny = x\nrhs = T(src)\n{}",
                tail.replace("OP", "x += rhs")
            ),
            "x += rhs",
            "ME list True 900000 {'i': 0, 'li': 0}\n",
            true,
        ),
        (
            format!(
                "{head}pad = bytearray(12000000)\nx = L(range(300000))\ny = x\nrhs = Idx()\n{}",
                tail.replace("OP", "x *= rhs")
            ),
            "x *= rhs",
            "ME L True 300000 {'i': 1, 'li': 0}\n",
            true,
        ),
        (callex.to_owned(), "f(*big, **kw)", "ME\n", false),
    ] {
        let run = |slice: Option<u64>, restore: bool, keep: bool, gc_yield: bool| {
            let e = engine();
            let out = Arc::new(Mutex::new(String::new()));
            let hook = |s: &mut Session| {
                attach(s, &out);
                s.set_gc_yield(gc_yield);
            };
            let total = u64::MAX / 2;
            let budget = slice.unwrap_or(u64::MAX / 4);
            let mut s = Session::new(&e).expect("session");
            hook(&mut s);
            s.set_fuel_total(Some(total));
            s.set_fuel(Some(budget));
            let mut p = s.start(&e.compile(&src, "wait.py").expect("compiles"));
            let mut parks = 0;
            while let Progress::Parked { .. } = p {
                parks += 1;
                assert!(
                    parks < 1_000,
                    "{op} {slice:?} {restore} {keep} {gc_yield}: {parks} parks"
                );
                if restore {
                    let bytes = s.snapshot().expect("snapshot");
                    s = e.restore(&bytes).expect("restore");
                    hook(&mut s);
                } else if keep {
                    s.snapshot().expect("snapshot");
                }
                s.set_fuel(Some(budget));
                p = s.resume(Resume::Continue).expect("resumes");
            }
            assert!(matches!(p, Progress::Complete(_)), "{op}: {p:?}");
            let text = out.lock().expect("output lock").clone();
            (text, total - s.fuel_total().expect("a total"))
        };
        let (got, fuel) = run(None, false, false, false);
        assert_eq!(got, want, "{op}: unparked");
        let modes = [
            (Some(100_003), false, false, false),
            (Some(100_003), true, false, false),
            (Some(100_003), false, true, false),
            (Some(50_003), false, false, false),
            (Some(50_003), false, false, true),
            (Some(50_003), true, false, true),
            (Some(50_003), false, true, true),
        ];
        std::thread::scope(|scope| {
            let runs: Vec<_> = modes
                .into_iter()
                .map(|m| {
                    let run = &run;
                    (m, scope.spawn(move || run(m.0, m.1, m.2, m.3)))
                })
                .collect();
            for (m, t) in runs {
                let (got, charged) = t.join().expect("run");
                assert_eq!(got, want, "{op} {m:?}");
                assert!(
                    !same_fuel || charged == fuel,
                    "{op} {m:?}: {charged} against {fuel}"
                );
            }
        });
    }
}

/// A set growing to a table of 4M slots or more gets its new keys unfilled
/// and marks them empty a slice per step before any item moves, so a park
/// can come with the new table part cleared and its progress past the old
/// table's size: resuming there, in the same session or a restored one,
/// must go on (it raised "bad native continuation" when the restore check
/// bounded the progress by the old table).
#[test]
fn a_park_while_a_big_set_table_is_cleared_resumes() {
    let src = "a = set(range(1_250_000))\nb = set(range(1_250_000, 2_500_000))\na |= b\nprint(len(a), min(a), max(a))";
    let (got, got_outcome, parks) = hopping(src, "fill.py", 200_000, 3);
    assert!(parks > 30, "{parks} parks");
    assert_eq!(got, "2500000 0 2499999\n");
    assert_eq!(got_outcome, "complete Some(\"None\")");
}

/// Over 32 MiB live, a collection marks a slice per safepoint (spec §9.2),
/// so a park, and a snapshot, can come in the middle of one: the snapshot
/// abandons the mark for a full collection, and the restored session marks
/// afresh. Tiny slices from any heap size make that happen in a small one,
/// with in-order and scattered writes into old objects (the second gives
/// up on its marks and backs off) and appends.
#[test]
fn a_snapshot_in_the_middle_of_an_incremental_collection_restores() {
    let src = "\
keep = [[i, str(i)] for i in range(20_000)]
for i in range(60_000):
    keep[i % 20_000][1] = [i]
    if i % 2:
        keep[(i * 7919) % 20_000][0] = (i,)
    if i % 3 == 0:
        keep.append([i])
    y = [i] * 40
print(len(keep), sum(len(k) for k in keep), keep[123], keep[-1])
";
    let (want, want_outcome) = plain(src, "incgc.py");
    let e = engine();
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = Session::new(&e).expect("session");
    attach(&mut s, &out);
    s.set_gc_incremental(64, 0);
    s.set_fuel(Some(100));
    let mut p = s.start(&e.compile(src, "incgc.py").expect("compiles"));
    let (mut marking, mut hops) = (0, 0);
    while let Progress::Parked { .. } = p {
        if s.gc_marking() {
            marking += 1;
            if marking % 2 == 0 {
                let mut r = e
                    .restore(&s.snapshot().expect("snapshot"))
                    .expect("restore");
                attach(&mut r, &out);
                r.set_gc_incremental(64, 0);
                s = r;
                hops += 1;
            }
        }
        s.set_fuel(Some(100));
        p = s.resume(Resume::Continue).expect("resumes");
    }
    assert!(hops >= 5, "{marking} parks in a mark, {hops} restores");
    let got = out.lock().expect("output lock").clone();
    assert_eq!((got, outcome(&p)), (want, want_outcome));
}

/// A fast-path call puts its callee's frame at the caller's argument
/// window, below the caller's dead temporaries, which a collection there
/// does not trace; a native that calls back into Python puts its frame
/// above all of the caller's registers. The collection once left those
/// temporaries' dead references in the stack, so the callback's frame (a
/// snapshot saves every register up to its top) held freed objects: the
/// snapshot was refused at restore as `corrupt (stack)` (found by the soak
/// on `conformance/own/300_re_differential.py` under a 1 MiB limit).
#[test]
fn a_collection_leaves_no_dead_reference_a_later_frame_exposes() {
    let cs = vec!["C()"; 30].join(", ");
    let zeros = vec!["0"; 24].join(", ");
    let src = format!(
        "\
import gc

class C:
    pass

def low(i):
    if i % 2:
        gc.collect()
        for _ in range(2):
            pass
    return 0

def key(v):
    if i % 2:
        checkpoint(v)
    return -v

for i in range(4):
    t = ({cs})
    t = None
    x = low(i)
    y = ({zeros}, sorted([3, 1, 2], key=key))
print('ok', y[-1])
"
    );
    let e = engine();
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = Session::new(&e).expect("session");
    attach(&mut s, &out);
    s.define(&HostModule::untyped("", &["checkpoint"]))
        .expect("defines");
    let mut p = s.start(&e.compile(&src, "window.py").expect("compiles"));
    let mut calls = 0;
    // The session itself runs on (a restore would not keep the dead
    // registers); each call's snapshot must restore.
    while let Progress::HostCall { .. } = p {
        calls += 1;
        let bytes = s.snapshot().expect("snapshot");
        if let Err(err) = e.restore(&bytes) {
            panic!("call {calls}: {err}");
        }
        p = s.resume(Resume::Value(HostValue::None)).expect("resumes");
    }
    assert_eq!(outcome(&p), "complete Some(\"None\")");
    assert_eq!(*out.lock().expect("output lock"), "ok [3, 2, 1]\n");
    assert_eq!(calls, 6);
}

/// A deep call grows the stack, which keeps its length once the call
/// returns; a restore caps the length at `STACK_SLACK` registers above the
/// live ones. The original then saved its full length and the restored
/// session the capped one, so their next snapshots differed.
#[test]
fn a_restored_session_saves_the_bytes_the_original_does_after_a_deep_call() {
    let mut locals = String::new();
    for i in 0..200 {
        writeln!(locals, "    a{i} = {i}").expect("string");
    }
    let src =
        format!("def f(n):\n{locals}    return f(n - 1) if n else 0\n\nf(700)\ncheckpoint()\n");
    let e = engine();
    let mut s = Session::new(&e).expect("session");
    s.define(&HostModule::untyped("", &["checkpoint"]))
        .expect("defines");
    let p = s.start(&e.compile(&src, "deep_call.py").expect("compiles"));
    assert!(matches!(p, Progress::HostCall { .. }), "{p:?}");
    let mut r = e
        .restore(&s.snapshot().expect("snapshot"))
        .expect("restore");
    assert!(
        r.snapshot().expect("snapshot") == s.snapshot().expect("snapshot"),
        "the restored session saves other bytes"
    );
}

const DEEP: &str = "\
class Walker:
    def __init__(self, n):
        self.n = n
    def items(self):
        def gen(k):
            for i in range(k):
                yield {'i': i, 's': str(i) * (i % 3)}
        total = 0
        for d in gen(self.n):
            total += d['i']
            if d['i'] % 50 == 0:
                print('at', d['i'], d['s'], total)
        return total

class Sub(Walker):
    def items(self):
        return [super().items(), sorted({x % 7 for x in range(self.n)}, key=lambda v: -v)]

w = Sub(400)
print(w.items())
try:
    print({}['missing'])
except KeyError as e:
    print('caught', repr(e))
w.items()
";

#[test]
fn a_pause_inside_a_generator_inside_a_method_survives_a_restore() {
    let (want, want_outcome) = plain(DEEP, "deep.py");
    for fuel in [3, 17, 100] {
        let (got, got_outcome, parks) = hopping(DEEP, "deep.py", fuel, 1);
        assert!(parks > 3, "fuel {fuel}: {parks} parks");
        assert_eq!(got, want, "fuel {fuel}");
        assert_eq!(got_outcome, want_outcome, "fuel {fuel}");
    }
}

#[test]
fn a_pause_inside_a_coroutine_driven_by_send_survives_a_restore() {
    // `send()` runs the coroutine's frame with a continuation that turns
    // its return into `StopIteration`; a restore at any park inside it
    // must keep that continuation.
    let src = "async def inner(n):\n    t = 0\n    for i in range(n):\n        t += i\n    return t\n\
               async def outer():\n    v = await inner(40)\n    try:\n        inner(30).send(None)\n    except StopIteration as e:\n        return v, e.value\n\
               try:\n    outer().send(None)\nexcept StopIteration as e:\n    print(e.value)\n";
    let (want, want_outcome) = plain(src, "coro.py");
    assert_eq!(want, "(780, 435)\n");
    for fuel in [1, 3] {
        let (got, got_outcome, parks) = hopping(src, "coro.py", fuel, 1);
        assert!(parks > 10, "fuel {fuel}: {parks} parks");
        assert_eq!(
            (got, got_outcome),
            (want.clone(), want_outcome.clone()),
            "fuel {fuel}"
        );
    }
}

#[test]
fn untaken_completion_text_survives_collection_and_restore() {
    use mm_compile::Options;
    use mm_engine::{Exit, Vm, VmConfig};

    let source = "class Text:\n    def __repr__(self):\n        return 'é' * 513\n    def __str__(self):\n        return '€' * 259\nText()";
    let program = mm_compile::compile(
        &mm_parse::parse(source).expect("parse"),
        Options {
            last_expr_value: true,
        },
    )
    .expect("compile");
    let mut vm = Vm::new(VmConfig::default()).expect("vm");
    vm.set_gc_stress(true);
    vm.set_want_completion_text(true);
    let entry = vm.load(&program, "text.py", source).expect("load");
    assert!(matches!(vm.run(entry), Exit::Complete(_)));
    let limit = vm.heap.memory_limit();
    let mut snapshot = Vec::new();
    vm.write_snapshot(&mut snapshot).expect("snapshot collects");
    let mut restored = Vm::read_snapshot(
        &mut mm_core::Reader::new(&snapshot),
        &[],
        None,
        &mut |_, _, _, _| None,
        limit,
    )
    .expect("restore");
    let mut again = Vec::new();
    restored.write_snapshot(&mut again).expect("collect again");
    assert_eq!(
        restored.take_completion_text(),
        Some(("é".repeat(513), "€".repeat(259)))
    );
    assert_eq!(restored.take_completion_text(), None);
    assert_eq!(
        vm.take_completion_text_capped(4),
        Some((("éé".into(), true), ("€".into(), true)))
    );
    assert_eq!(vm.take_completion_text(), None);
}

#[test]
fn module_catalog_storage_is_budgeted_before_creation_and_restore() {
    use mm_core::{HeapConfig, Reader};
    use mm_engine::{SnapError, StdModule, Vm, VmConfig};

    // The catalog itself is caller-owned. Each session owns code ranges and
    // globals for its entries, even when no module has been imported.
    static MODULES: [StdModule; 131_072] = [StdModule {
        name: "unused",
        code: &[],
        packing: None,
    }; 131_072];
    let config = |memory_limit| VmConfig {
        heap: HeapConfig {
            memory_limit,
            ..HeapConfig::default()
        },
        stdlib: &MODULES,
        ..VmConfig::default()
    };
    assert!(Vm::new(config(3 << 20)).is_err());
    let mut vm = Vm::new(config(8 << 20)).expect("roomy catalog");
    let limit = vm.heap.memory_limit() - vm.heap.available_bytes();
    vm.heap.set_memory_limit(limit);
    let mut snapshot = Vec::new();
    vm.write_snapshot(&mut snapshot).expect("snapshot");
    assert!(matches!(
        Vm::read_snapshot(
            &mut Reader::new(&snapshot),
            &MODULES,
            None,
            &mut |_, _, _, _| None,
            3 << 20
        ),
        Err(SnapError::TooLarge)
    ));
    let mut restored = Vm::read_snapshot(
        &mut Reader::new(&snapshot),
        &MODULES,
        None,
        &mut |_, _, _, _| None,
        limit,
    )
    .expect("same-budget restore");
    assert_eq!(restored.heap.memory_limit(), limit);
    let mut again = Vec::new();
    restored.write_snapshot(&mut again).expect("snapshot again");
    // Both writers run GC, whose counters are part of the image.
    snapshot.clear();
    vm.write_snapshot(&mut snapshot)
        .expect("second original snapshot");
    assert!(snapshot == again, "restored catalog changed the image");
}

#[test]
fn every_own_case_survives_restores_at_parks() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/own");
    let mut cases: Vec<_> = std::fs::read_dir(&root)
        .expect("conformance/own exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .collect();
    cases.sort();
    let mut total = 0;
    // The cases on every core (about 18 minutes on one in a debug build),
    // checked in name order.
    let runs = on_every_core(&cases, |case| {
        let src = std::fs::read_to_string(case).expect("readable");
        let name = case.to_string_lossy();
        // About 20 restores per case, at parks every 97 units.
        let (_, _, n) = hopping(&src, &name, 97, usize::MAX);
        (plain(&src, &name), hopping(&src, &name, 97, n / 20 + 1))
    });
    for (case, ((want, want_outcome), (got, got_outcome, parks))) in cases.iter().zip(runs) {
        let name = case.to_string_lossy();
        assert_eq!(got, want, "{name}");
        assert_eq!(got_outcome, want_outcome, "{name}");
        total += parks;
    }
    assert!(total > 100, "{total} parks");
}

#[test]
fn host_calls_and_waits_resume_after_a_restore_with_the_same_ids() {
    let src = "\
import asyncio
from contextlib import aclosing
def show(x):
    print('got', x)
    return x
a = show(fetch({'k': 1}))
closed = []
async def stream(x):
    try:
        yield await slow(x)
    finally:
        closed.append(x)
async def one(x):
    async with aclosing(stream(x)) as values:
        async for value in values:
            return value
async def main():
    result = await asyncio.gather(one('a'), one('b'))
    assert sorted(closed) == ['a', 'b']
    return result
r = asyncio.run(main())
print(a, r)
r
";
    let e = engine();
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = Session::new(&e).expect("session");
    s.define(&HostModule::untyped("", &["fetch", "slow"]))
        .expect("defines");
    attach(&mut s, &out);
    let p = s.start(&e.compile(src, "host.py").expect("compiles"));
    let Progress::HostCall { call, name, .. } = p else {
        panic!("{p:?}");
    };
    assert_eq!(name, "fetch");
    let mut s = e
        .restore(&s.snapshot().expect("snapshot"))
        .expect("restore");
    attach(&mut s, &out);
    // The restored session waits for the same answer.
    assert!(s.resume(Resume::Continue).is_err());
    let mut p = s.resume(Resume::Value(HostValue::Int(7))).expect("resumes");
    let mut futures = Vec::new();
    while let Progress::HostCall { call: c, .. } = p {
        assert!(c > call);
        futures.push(c);
        s = e
            .restore(&s.snapshot().expect("snapshot"))
            .expect("restore");
        attach(&mut s, &out);
        p = s.resume(Resume::Future).expect("resumes");
    }
    let Progress::Waiting { calls } = p else {
        panic!("{p:?}");
    };
    assert_eq!(calls, futures);
    let mut s = e
        .restore(&s.snapshot().expect("snapshot"))
        .expect("restore");
    attach(&mut s, &out);
    let batch = calls
        .iter()
        .map(|&c| (c, Ok(HostValue::Str(format!("r{c}")))))
        .collect();
    let p = s.resume(Resume::Batch(batch)).expect("resumes");
    assert_eq!(
        outcome(&p),
        format!("complete Some(\"['r{}', 'r{}']\")", calls[0], calls[1])
    );
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        format!("got 7\n7 ['r{}', 'r{}']\n", calls[0], calls[1])
    );
}

/// `Session::pending` re-reports the stop a session is at (a host call
/// with its arguments, a wait on futures, a park), and a session restored
/// in a new process (this test binary again, running `pending_in_a_child`)
/// reports the stop its image was saved at.
#[test]
fn pending_reports_the_stop_across_a_restore_in_a_new_process() {
    let e = engine();
    let mut s = Session::new(&e).expect("session");
    s.define(&HostModule::untyped("", &["fetch", "slow"]))
        .expect("defines");
    let mut stops = Vec::new();
    let mut save = |s: &mut Session, p: Option<&Progress>| {
        assert_eq!(s.pending().as_ref(), p);
        stops.push((s.snapshot().expect("snapshot"), format!("{p:?}")));
    };
    save(&mut s, None);
    let src = "\
import asyncio
x = fetch(1, 'two', [2.5, None, (b'\\x00',)], k={'a': 10 ** 30})
async def main():
    return await asyncio.gather(slow(1), slow(2))
asyncio.run(main())
";
    let p = s.start(&e.compile(src, "p.py").expect("compiles"));
    let Progress::HostCall { args, kwargs, .. } = &p else {
        panic!("{p:?}");
    };
    assert_eq!(args.len(), 3);
    assert_eq!(kwargs[0].0, "k");
    save(&mut s, Some(&p));
    let mut p = s.resume(Resume::Value(HostValue::None)).expect("resumes");
    while let Progress::HostCall { .. } = p {
        save(&mut s, Some(&p));
        p = s.resume(Resume::Future).expect("resumes");
    }
    let Progress::Waiting { calls } = p.clone() else {
        panic!("{p:?}");
    };
    save(&mut s, Some(&p));
    let answer = |c: u64| (c, Ok(HostValue::Int(0)));
    let p = s
        .resume(Resume::Batch(vec![answer(calls[0])]))
        .expect("resumes");
    assert_eq!(
        p,
        Progress::Waiting {
            calls: vec![calls[1]]
        }
    );
    save(&mut s, Some(&p));
    let p = s
        .resume(Resume::Batch(vec![answer(calls[1])]))
        .expect("resumes");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    save(&mut s, None);
    s.set_fuel(Some(500));
    let p = s.start(&e.compile("while True: pass", "spin.py").expect("compiles"));
    assert!(matches!(p, Progress::Parked { .. }), "{p:?}");
    save(&mut s, Some(&p));

    let dir = std::env::temp_dir().join(format!("mm-pending-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for (i, (image, _)) in stops.iter().enumerate() {
        std::fs::write(dir.join(format!("{i}.img")), image).expect("write");
    }
    let child = std::process::Command::new(std::env::current_exe().expect("this test"))
        .args(["pending_in_a_child", "--exact", "--nocapture"])
        .env("MM_PENDING_IMAGES", &dir)
        .output()
        .expect("runs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(child.status.success(), "{child:?}");
    let reported: Vec<String> = String::from_utf8_lossy(&child.stdout)
        .lines()
        .filter_map(|l| l.strip_prefix("pending: ").map(str::to_owned))
        .collect();
    let want: Vec<String> = stops.into_iter().map(|(_, p)| p).collect();
    assert_eq!(reported, want);
}

/// The new process of the test above: restores each image in
/// `MM_PENDING_IMAGES` on a new Engine and prints what `pending` reports.
#[test]
fn pending_in_a_child() {
    let Some(dir) = std::env::var_os("MM_PENDING_IMAGES") else {
        return;
    };
    let e = engine();
    let dir = std::path::Path::new(&dir);
    for i in 0.. {
        let Ok(image) = std::fs::read(dir.join(format!("{i}.img"))) else {
            break;
        };
        let s = e.restore(&image).expect("restores");
        println!("pending: {:?}", s.pending());
    }
}

#[test]
fn suspended_task_stacks_keep_their_budget_across_restore() {
    let e = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(8 << 20)));
    let small = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(3 << 20)));
    let mut locals = String::new();
    for i in 0..200 {
        writeln!(locals, "    a{i} = {i}").expect("string");
    }
    let code = e.compile(&format!(
        "import asyncio\nasync def f(n, k):\n{locals}    if n == 350:\n        return await checkpoint(k)\n    return await f(n + 1, k)\nasync def main():\n    return await asyncio.gather(f(0, 3), f(0, 7), f(0, 11))\nasyncio.run(main())"
    ), "tasks.py").expect("compiles");
    let mut s = Session::new(&e).expect("session");
    s.define(&HostModule::untyped("", &["checkpoint"]))
        .expect("defines");
    let mut p = s.start(&code);
    let mut calls = Vec::new();
    while let Progress::HostCall { call, args, .. } = p {
        calls.push((call, Ok(args[0].clone())));
        s = e
            .restore(&s.snapshot().expect("snapshot"))
            .expect("restore at call");
        p = s.resume(Resume::Future).expect("future");
    }
    let Progress::Waiting { calls: waiting } = p else {
        panic!("{p:?}");
    };
    assert_eq!(waiting, calls.iter().map(|(id, _)| *id).collect::<Vec<_>>());
    assert_eq!(calls.len(), 3);
    let snapshot = s.snapshot().expect("snapshot at wait");
    assert!(matches!(
        small.restore(&snapshot),
        Err(SnapshotError::TooLarge)
    ));
    s = e.restore(&snapshot).expect("restore at wait");
    let p = s.resume(Resume::Batch(calls)).expect("batch");
    assert_eq!(outcome(&p), "complete Some(\"[3, 7, 11]\")");

    // Completion, cancellation and reset release suspended execution storage.
    let allocate = e
        .compile("len('x' * (5 << 20))", "allocate.py")
        .expect("compiles");
    assert_eq!(outcome(&s.start(&allocate)), "complete Some(\"5242880\")");
    for reset in [false, true] {
        let mut s = e.restore(&snapshot).expect("restore at wait");
        if reset {
            s.reset().expect("reset");
        } else {
            assert!(s.cancel());
        }
        assert_eq!(outcome(&s.start(&allocate)), "complete Some(\"5242880\")");
    }
}

#[test]
fn a_session_between_cells_keeps_its_globals_and_host_functions() {
    let e = engine();
    let mut s = Session::new(&e).expect("session");
    s.define(&HostModule::untyped("", &["ask"]))
        .expect("defines");
    s.define(&HostModule {
        constants: [("DEEP", deep_value(50))]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
        ..HostModule::untyped("tools", &["get"])
    })
    .expect("defines");
    let p = s.start(
        &e.compile("x = [1, 2]\nclass C:\n    k = 3\n", "a.py")
            .expect("ok"),
    );
    assert_eq!(outcome(&p), "complete None");
    let mut r = e
        .restore(&s.snapshot().expect("snapshot"))
        .expect("restore");
    let code = e
        .compile(
            "import tools\nx.append(C.k)\n(x, len(str(tools.DEEP)))",
            "b.py",
        )
        .expect("ok");
    let want = outcome(&s.start(&code));
    assert!(want.starts_with("complete Some(\"([1, 2, 3], "), "{want}");
    assert_eq!(outcome(&r.start(&code)), want);
    let p = r.start(&e.compile("ask(1)", "c.py").expect("ok"));
    assert!(
        matches!(p, Progress::HostCall { ref name, .. } if name == "ask"),
        "{p:?}"
    );
}

/// `TZif` v2 data with no transitions whose footer is the POSIX rule `rule`.
fn tzif_rule(rule: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..2 {
        out.extend_from_slice(b"TZif2");
        out.extend_from_slice(&[0; 15]);
        for count in [0u32, 0, 0, 0, 1, 4] {
            out.extend_from_slice(&count.to_be_bytes());
        }
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(b"UTC\0");
    }
    out.push(b'\n');
    out.extend_from_slice(rule.as_bytes());
    out.push(b'\n');
    out
}

#[test]
fn the_host_time_zone_and_its_dst_rules_survive_restore() {
    let e = engine();
    let code = e
        .compile(
            "import time\n\
             s, w = time.localtime(1718454600), time.localtime(1704067200)\n\
             (s.tm_hour, s.tm_isdst, s.tm_zone, w.tm_hour, w.tm_isdst, w.tm_zone, time.tzname)",
            "z.py",
        )
        .expect("ok");
    let mut utc = Session::new(&e).expect("session");
    assert_eq!(
        outcome(&utc.start(&code)),
        "complete Some(\"(12, 0, 'UTC', 0, 0, 'UTC', ('UTC', 'UTC'))\")"
    );
    // Saved before `time` is imported: the restored session must not need
    // the zone again.
    let mut s = Session::new(&e).expect("session");
    s.set_local_timezone(Some(tzif_rule("GMT0BST,M3.5.0/1,M10.5.0")));
    let mut r = e
        .restore(&s.snapshot().expect("snapshot"))
        .expect("restore");
    let want = "complete Some(\"(13, 1, 'BST', 0, 0, 'GMT', ('GMT', 'BST'))\")";
    assert_eq!(outcome(&s.start(&code)), want);
    assert_eq!(outcome(&r.start(&code)), want);
}

#[test]
fn snapshot_output_capacity_is_bounded_without_losing_a_pending_call() {
    let memory = 7 << 20;
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)));
    let mut session = Session::new(&engine).expect("session");
    session
        .define(&HostModule::untyped("", &["checkpoint"]))
        .expect("defines");
    let code = engine
        .compile(
            "chunks = [str(i) + 'x' * (1 << 17) for i in range(32)]\ncheckpoint(len(chunks))",
            "snapshot_limit.py",
        )
        .expect("compiles");
    assert!(matches!(session.start(&code), Progress::HostCall { .. }));
    let image = session.snapshot().expect("default limit");
    // Ordinary Vec growth previously retained 8 MiB under this 7 MiB cap.
    assert!(image.capacity() as u64 <= memory);
    let size = image.len() as u64;
    for limit in [0, 53, size - 1] {
        assert_eq!(
            session.snapshot_with_limit(limit).err(),
            Some(SnapshotError::TooLarge)
        );
    }
    // Exact fit includes the checksum, not just the payload. A failed attempt
    // must neither poison the session nor consume its pending host call.
    for limit in [size, size + 17] {
        let image = session.snapshot_with_limit(limit).expect("fits");
        assert_eq!(image.len() as u64, size);
        assert!(image.capacity() as u64 <= limit);
        let mut restored = engine.restore(&image).expect("restore");
        let progress = restored
            .resume(Resume::Value(HostValue::Int(47)))
            .expect("answer");
        assert_eq!(outcome(&progress), "complete Some(\"47\")");
        let check = engine
            .compile("(len(chunks), len(chunks[31]))", "check.py")
            .expect("compiles");
        assert_eq!(
            outcome(&restored.start(&check)),
            "complete Some(\"(32, 131074)\")"
        );
    }
    let progress = session
        .resume(Resume::Value(HostValue::Int(59)))
        .expect("original answer");
    assert_eq!(outcome(&progress), "complete Some(\"59\")");
}

fn deep_value(n: usize) -> HostValue {
    let mut v = HostValue::Dict(vec![("leaf".to_owned(), HostValue::Int(1))]);
    for _ in 0..n {
        v = HostValue::List(vec![v, HostValue::None]);
    }
    v
}

#[test]
fn malformed_snapshots_are_typed_errors() {
    let e = engine();
    let mut s = Session::new(&e).expect("session");
    s.set_fuel(Some(50));
    let p = s.start(&e.compile(DEEP, "deep.py").expect("ok"));
    assert!(matches!(p, Progress::Parked { .. }));
    let bytes = s.snapshot().expect("snapshot");
    assert!(e.restore(&bytes).is_ok());
    for cut in (0..bytes.len()).step_by(bytes.len() / 300 + 1) {
        assert!(e.restore(&bytes[..cut]).is_err(), "cut at {cut}");
    }
    let mut other = bytes.clone();
    other[12] ^= 1;
    let sum = mm_core::xxh64(&other[..other.len() - 8], 0);
    let n = other.len();
    other[n - 8..].copy_from_slice(&sum.to_le_bytes());
    let other_id = micro_monty::image_build_id(&other).expect("an image");
    assert_ne!(&other_id, micro_monty::build_id());
    assert_eq!(
        e.restore(&other).err(),
        Some(SnapshotError::Incompatible {
            format: micro_monty::SNAPSHOT_FORMAT,
            build_id: other_id,
        })
    );
    // The message an embedder shows names both builds by their first bytes.
    let hex = |id: &[u8; 32]| {
        id[..8].iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    };
    assert_eq!(
        e.restore(&other).err().map(|err| err.to_string()),
        Some(format!(
            "the snapshot was written by another micro-monty build (format {f}, build {}...; \
             this is format {f}, build {}...)",
            hex(&other_id),
            hex(micro_monty::build_id()),
            f = micro_monty::SNAPSHOT_FORMAT,
        ))
    );
    // Another format version is incompatible before its checksum counts.
    let mut older = bytes.clone();
    older[8..10].copy_from_slice(&3u16.to_le_bytes());
    assert_eq!(
        e.restore(&older).err(),
        Some(SnapshotError::Incompatible {
            format: 3,
            build_id: *micro_monty::build_id(),
        })
    );
    assert_eq!(
        micro_monty::image_build_id(b"not an image at all, no"),
        None
    );
    let mut flipped = bytes.clone();
    flipped[bytes.len() / 2] ^= 0x40;
    assert_eq!(
        e.restore(&flipped).err(),
        Some(SnapshotError::Corrupt {
            what: "checksum".into()
        })
    );
    assert!(matches!(
        e.restore(b"not a snapshot at all"),
        Err(SnapshotError::Corrupt { .. })
    ));
    let small = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(64 << 10)));
    assert_eq!(small.restore(&bytes).err(), Some(SnapshotError::TooLarge));
}

/// A zone source over the system's tz database that counts its calls.
fn system_zones(calls: &Arc<Mutex<Vec<String>>>) -> micro_monty::ZoneSource {
    let calls = Arc::clone(calls);
    Box::new(move |key: &str| {
        calls.lock().expect("calls lock").push(key.to_owned());
        std::fs::read(std::path::Path::new("/usr/share/zoneinfo").join(key)).ok()
    })
}

#[test]
fn loaded_zones_survive_a_restore_without_the_zone_source() {
    let e = engine();
    let out = Arc::new(Mutex::new(String::new()));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut s = Session::new(&e).expect("session");
    attach(&mut s, &out);
    s.set_zone_source(Some(system_zones(&calls)));
    let first = "\
from zoneinfo import ZoneInfo
from datetime import datetime
paris = ZoneInfo('Europe/Paris')
summer = datetime(2026, 7, 1, 12, tzinfo=paris)
print(summer.utcoffset(), ZoneInfo('Europe/Paris') is paris)
import _mm
print([_mm.tzif(k) for k in ('/etc/passwd', '../etc/passwd', 'Europe//Paris', 'Europe/./Paris', '')])
";
    let p = s.start(&e.compile(first, "a.py").expect("compiles"));
    assert_eq!(outcome(&p), "complete Some(\"None\")");
    assert_eq!(*calls.lock().expect("calls lock"), ["Europe/Paris"]);
    // The restored session has no zone source: Paris comes from the cache
    // with the same rules; a new key says the host provides no data.
    let mut r = e
        .restore(&s.snapshot().expect("snapshot"))
        .expect("restores");
    attach(&mut r, &out);
    let second = "\
print(ZoneInfo('Europe/Paris') is paris, datetime(2026, 12, 1, tzinfo=paris).tzname())
print(datetime(2099, 7, 1, tzinfo=ZoneInfo('Europe/Paris')).utcoffset())
try:
    ZoneInfo('Asia/Tokyo')
except KeyError as e:
    print(type(e).__name__, e)
print(ZoneInfo('UTC').utcoffset(None))
";
    let p = r.start(&e.compile(second, "b.py").expect("compiles"));
    assert_eq!(outcome(&p), "complete Some(\"None\")");
    assert_eq!(*calls.lock().expect("calls lock"), ["Europe/Paris"]);
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "2:00:00 True\n[None, None, None, None, None]\nTrue CET\n2:00:00\nZoneInfoNotFoundError 'No time zone found with key \
         Asia/Tokyo (this sandbox has no time zone data: the host provides none; \
         datetime.timezone works for fixed offsets)'\n0:00:00\n"
    );
}

#[path = "../../../fuzz/fuzz_targets/snapshot_restore_input.rs"]
mod fuzz_input;

/// The `snapshot_restore` fuzz target's stored crash and out-of-memory
/// seeds, then every byte of each program's VM-state tail near its end,
/// where the frames, continuations, tasks and shapes are, inverted in turn:
/// restore must refuse each with an error or run it without panicking or
/// allocating beyond the limit. With the frame-window or shape-table check
/// weakened, some of the tail edits restored and then panicked. Edits at
/// fixed offsets elsewhere stop reaching their field whenever the compiler,
/// the prelude or the heap layout changes; mm-engine's snapshot tests
/// corrupt the decoded fields instead (references, dict indexes, class size
/// hints and the rest).
#[test]
fn fuzzed_images_restore_to_errors_not_panics() {
    static PANICS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../conformance/breadth/fuzz/artifacts"
    );
    let mut seeds: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .expect("artifacts")
        .map(|e| e.expect("entry").path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("snapshot_restore-"))
        })
        .map(|p| (p.display().to_string(), std::fs::read(&p).expect("seed")))
        .collect();
    let n_seeds = seeds.len();
    for pick in (0..6).chain([0xFE]) {
        for pos in 8192 - 1536..8192u16 {
            let [lo, hi] = pos.to_le_bytes();
            let data = vec![0x04, pick, lo, hi, 0xFF, 0x00];
            seeds.push((format!("tail of program {pick}: {data:02x?}"), data));
        }
    }
    assert!(n_seeds >= 9, "seeds: {n_seeds}");
    // The facade contains panics (an unwinding build turns them into
    // errors), so the hook is what sees them, as an aborting fuzz build would.
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        PANICS.lock().expect("panics").push(info.to_string());
        default(info);
    }));
    let mut failed = Vec::new();
    for (name, data) in &seeds {
        // Peak memory, where Linux reports it: the session's limit is
        // 64 MiB, so a seed must not grow it by gigabytes.
        let _ = std::fs::write("/proc/self/clear_refs", "5");
        let before = peak_rss_kb();
        let _ = std::panic::catch_unwind(|| fuzz_input::run(data));
        let mut p = PANICS.lock().expect("panics");
        if !p.is_empty() {
            failed.push(format!("{name}: {}", p.join("; ")));
            p.clear();
        }
        if let (Some(a), Some(b)) = (before, peak_rss_kb())
            && b.saturating_sub(a) > 512 << 10
        {
            failed.push(format!("{name}: peak memory grew {} MiB", (b - a) >> 10));
        }
    }
    drop(std::panic::take_hook());
    assert!(failed.is_empty(), "{failed:#?}");
}

fn peak_rss_kb() -> Option<usize> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// One settings value for a new session, a restore and a template's
/// instances: where output goes, the wall clock, entropy, the zone source,
/// the text cut and `int_bits_text`, none of which an image carries.
fn host_settings(out: &Arc<Mutex<String>>, zones: &Arc<Mutex<Vec<String>>>) -> SessionSettings {
    let mut s = SessionSettings::default();
    let sink = Arc::clone(out);
    s.output = Some(Box::new(move |stream, t: &str| {
        let mut o = sink.lock().expect("output lock");
        let _ = write!(o, "{stream:?}:{t}");
    }));
    s.wall_clock = Some(Box::new(|| 1_234_567_890.5));
    s.entropy = Some(Box::new(|b: &mut [u8]| b.fill(0xab)));
    let calls = Arc::clone(zones);
    s.zone_source = Some(Box::new(move |key: &str| {
        calls.lock().expect("zones lock").push(key.to_owned());
        None
    }));
    s.text_cut = TextCut::HeadTail;
    s.int_bits_text = true;
    s
}

/// The rest of the cell after its host call, then a cell whose value is a
/// 16,610-bit int: what the settings show.
fn finish_with_settings(s: &mut Session, e: &Engine, out: &Arc<Mutex<String>>) -> String {
    let p = s.resume(Resume::Value(HostValue::None)).expect("resumes");
    let first = outcome(&p);
    let p = s.start(&e.compile("10 ** 5000", "big.py").expect("compiles"));
    let text = std::mem::take(&mut *out.lock().expect("output lock"));
    format!("{text}{first}\n{}", outcome(&p))
}

#[test]
fn restore_with_and_instantiate_with_apply_the_hosts_settings() {
    const CELL: &str = "\
import time, os
from zoneinfo import ZoneInfo
wait()
print(time.time(), os.urandom(3).hex())
try:
    ZoneInfo('Mars/Olympus')
except KeyError as e:
    print(type(e).__name__, e)
'x' * 100 + 'END'
";
    let sinks = || {
        (
            Arc::new(Mutex::new(String::new())),
            Arc::new(Mutex::new(Vec::<String>::new())),
        )
    };
    let engine = engine();
    let (out, zones) = sinks();
    let mut first = Session::new_with(&engine, host_settings(&out, &zones)).expect("session");
    first.set_text_limit(64);
    first
        .define(&HostModule::untyped("", &["wait"]))
        .expect("defines");
    let code = engine.compile(CELL, "c.py").expect("compiles");
    let stop = first.start(&code);
    assert!(matches!(stop, Progress::HostCall { .. }), "{stop:?}");
    let image = first.snapshot().expect("snapshot");
    let expected = finish_with_settings(&mut first, &engine, &out);
    // What each setting shows: output through the callback, the pinned
    // clock and entropy, the zone source asked (an unknown key, not "no
    // data"), both ends of the long str, and the int's bit count instead
    // of CPython's ValueError.
    for shown in [
        "Stdout:1234567890.5 ababab\n",
        "ZoneInfoNotFoundError 'No time zone found with key Mars/Olympus'",
        "xxxEND'",
        "<int with 16610 bits>",
    ] {
        assert!(expected.contains(shown), "{shown:?} not in {expected}");
    }
    assert_eq!(*zones.lock().expect("zones lock"), ["Mars/Olympus"]);

    let (out, zones) = sinks();
    let mut restored = engine
        .restore_with(&image, host_settings(&out, &zones))
        .expect("restores");
    assert_eq!(finish_with_settings(&mut restored, &engine, &out), expected);
    assert_eq!(*zones.lock().expect("zones lock"), ["Mars/Olympus"]);

    // A template's instances take them the same way.
    let template = micro_monty::SessionTemplate::capture(&engine, &mut restored).expect("captures");
    let (out, zones) = sinks();
    let mut instance = template
        .instantiate_with(host_settings(&out, &zones))
        .expect("instantiates");
    let stop = instance.start(&code);
    assert!(matches!(stop, Progress::HostCall { .. }), "{stop:?}");
    assert_eq!(finish_with_settings(&mut instance, &engine, &out), expected);
}

/// The first word of the live `NSTATE` object of chunked operation `op`
/// (`vals` value slots, then `raw` words, the first `op`) in `image`.
fn nstate_words(image: &[u8], op: u8, vals: usize, raw: usize) -> Option<usize> {
    let n = vals + raw;
    let at = 8 + 8 * vals;
    (0..image.len().saturating_sub(8 + 8 * n)).find_map(|i| {
        let h = &image[i..i + 8];
        (h[..2] == 54u16.to_le_bytes()
            && usize::from(h[3]) == vals
            && h[4..] == u32::try_from(n).unwrap_or(0).to_le_bytes()
            && image[i + at..i + at + 8] == u64::from(op).to_le_bytes())
        .then_some(i + 8)
    })
}

/// Snapshots of each park inside chunked operation `op` (`vals` value
/// slots, `raw` words) while `stmt` runs after `setup`, with where the
/// state's words start: a park after each step (a step charges about
/// 2^17 units).
fn parks_inside(
    eng: &Engine,
    (setup, stmt, op, vals, raw): (&str, &str, u8, usize, usize),
) -> Vec<(Vec<u8>, usize)> {
    let mut s = Session::new(eng).expect("session");
    let p = s.start(&eng.compile(setup, "setup.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{setup}: {p:?}");
    s.set_fuel(Some(50_000));
    let mut p = s.start(&eng.compile(stmt, "stmt.py").expect("compiles"));
    let mut images = Vec::new();
    while let Progress::Parked { .. } = p {
        let image = s.snapshot().expect("snapshot");
        if let Some(at) = nstate_words(&image, op, vals, raw) {
            images.push((image, at));
        }
        s.set_fuel(Some(50_000));
        p = s.resume(Resume::Continue).expect("resumes");
    }
    assert!(matches!(p, Progress::Complete(_)), "{stmt}: {p:?}");
    assert!(!images.is_empty(), "{setup}: never parked inside op {op}");
    images
}

/// How image `img` goes on when restored: refused, an error, complete,
/// panicked, or hangs (still parking on fuel after 64 slices of 2^20
/// units, far past what the operation needs).
fn forged_run(eng: &Engine, img: &[u8]) -> String {
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Ok(mut restored) = eng.restore(img) else {
            return "refused".to_owned();
        };
        for _ in 0..64 {
            restored.set_fuel(Some(1 << 20));
            match restored.resume(Resume::Continue) {
                Ok(Progress::Parked { .. }) => {}
                Ok(Progress::Complete(_)) => return "complete".to_owned(),
                Ok(Progress::Error(e)) => return format!("error {}", e.message),
                other => return format!("{other:?}"),
            }
        }
        "hangs".to_owned()
    }));
    run.unwrap_or_else(|_| "panicked".to_owned())
}

/// A forged image cannot crash or hang json's and unicodedata's steps
/// (`JSON_SCAN`, `JSON_DUMP`, `UNI_NORM`): each step checks every word its
/// state saved against the data before using it, so a word no step leaves
/// is the usual `SystemError`, or at worst a different result. Every word
/// of the state is forged at a few parks inside each kind of step. The
/// review of eb78933 aborted the process with a `JSON_SCAN` position of
/// `u64::MAX` (a slice past the input); a fill position inside an escape,
/// or its end past the input, looped forever in one step.
#[test]
fn forged_json_and_unicodedata_step_states_are_errors() {
    // Setup, statement, and the op with its value slots and raw words (the
    // op's included), as in `nstep::STEPS`.
    const SCAN: (u8, usize, usize) = (15, 5, 16);
    let eng = engine();
    let mut cases = vec![
        (
            "import json\ns = json.dumps([list(range(40)), {'a': 1.5, 'b': 'x\\ny'}] * 3_000)",
            "x = json.loads(s)",
            SCAN,
        ),
        // An escaped backslash, then `a`: a position one off lands inside
        // an escape (`\a`).
        (
            "import json\ns = '[\"' + '\\\\\\\\a' * 1_000_000 + '\", 1]'",
            "x = json.loads(s)",
            SCAN,
        ),
        (
            "import json\ns = '[-1.' + '5' * 3_000_000 + 'e-3, 2]'",
            "x = json.loads(s)",
            SCAN,
        ),
        (
            "import json\ns = '[' + '7' * 2_000_000 + '.5]'",
            "x = json.loads(s, parse_float=len)",
            SCAN,
        ),
        (
            "import json\nd = {str(i): i for i in range(70_000)}\ns = json.dumps(d)",
            "x = json.loads(s)",
            SCAN,
        ),
        (
            "import json\nxs = [list(range(30)), 'a\\xe9' * 100_000] * 6",
            "x = json.dumps(xs, indent=1)",
            (16, 6, 6),
        ),
    ];
    if cfg!(feature = "mod-unicodedata") {
        cases.push((
            "import unicodedata\ns = 'e\\u0301\\xe9' * 150_000",
            "x = unicodedata.normalize('NFC', s)",
            (17, 3, 10),
        ));
    }
    let mut bad = Vec::new();
    let mut forged = 0;
    for (setup, stmt, (op, vals, raw)) in cases {
        let images = parks_inside(&eng, (setup, stmt, op, vals, raw));
        let picks = [0, images.len() / 2, images.len() - 1];
        for k in picks
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
        {
            let (image, at) = &images[k];
            let word = |i: usize| {
                let b = &image[at + 8 * i..at + 8 * i + 8];
                u64::from_le_bytes(b.try_into().unwrap_or_default())
            };
            for i in (0..vals + raw).filter(|&i| i != vals) {
                let w = word(i);
                let mut values = vec![u64::MAX, 1 << 33, w.wrapping_add(1), w.wrapping_sub(1)];
                if i < vals {
                    // Another slot's value in this one.
                    values = (0..vals).filter(|&j| j != i).map(word).collect();
                    values.push(word(vals + 1));
                }
                for v in values.into_iter().filter(|&v| v != w) {
                    let mut img = image.clone();
                    img[at + 8 * i..at + 8 * i + 8].copy_from_slice(&v.to_le_bytes());
                    let end = img.len() - 8;
                    let sum = mm_core::xxh64(&img[..end], 0);
                    img[end..].copy_from_slice(&sum.to_le_bytes());
                    forged += 1;
                    let run = forged_run(&eng, &img);
                    // The scan's position past the input, at its first park.
                    let past = k == 0 && i == vals + 1 && v == u64::MAX && op == 15;
                    if run == "panicked"
                        || run == "hangs"
                        || (past && !run.contains("bad native continuation"))
                    {
                        bad.push(format!("{stmt} park {k} word {i}: {w:#x} -> {v:#x}: {run}"));
                    }
                }
            }
        }
    }
    eprintln!("forged {forged} step states");
    assert!(
        bad.is_empty(),
        "{} of {forged}:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// A chunked native's state marked finished (`nstep::op::DONE`, 255),
/// which restore does not check, is never stepped: one forged on a live
/// state raises the bad continuation `SystemError` at its next step. Each
/// op whose module the engine's own tests lack (`snapshot_tests.rs` covers
/// the rest), including a dump with sorted keys, which goes on in
/// `_json_steps` after sorting them and so is not marked when it hands
/// them over.
#[test]
fn a_live_step_state_forged_finished_raises() {
    let eng = engine();
    let mut cases = vec![
        (
            "import math\nxs = [0.1] * 5_000_000",
            "x = math.fsum(xs)",
            (13, 3, 11),
        ),
        (
            "import json\ns = json.dumps([list(range(40))] * 3_000)",
            "x = json.loads(s)",
            (15, 5, 16),
        ),
        (
            "import json\nxs = [list(range(30)), 'a\\xe9' * 100_000] * 6",
            "x = json.dumps(xs, indent=1)",
            (16, 6, 6),
        ),
        (
            "import json\nd = {str(i): i for i in range(300_000)}",
            "x = json.dumps(d, sort_keys=True)",
            (16, 6, 6),
        ),
    ];
    if cfg!(feature = "mod-unicodedata") {
        cases.push((
            "import unicodedata\ns = 'e\\u0301\\xe9' * 150_000",
            "x = unicodedata.normalize('NFC', s)",
            (17, 3, 10),
        ));
    }
    let mut runs = 0;
    for (setup, stmt, (op, vals, raw)) in cases {
        let images = parks_inside(&eng, (setup, stmt, op, vals, raw));
        let picks: std::collections::BTreeSet<_> = [0, images.len() / 2, images.len() - 1].into();
        for k in picks {
            let (image, at) = &images[k];
            let mut img = image.clone();
            img[at + 8 * vals..at + 8 * vals + 8].copy_from_slice(&255u64.to_le_bytes());
            let end = img.len() - 8;
            let sum = mm_core::xxh64(&img[..end], 0);
            img[end..].copy_from_slice(&sum.to_le_bytes());
            let run = forged_run(&eng, &img);
            assert!(
                run.contains("bad native continuation"),
                "{stmt} park {k} of {}: {run}",
                images.len()
            );
            runs += 1;
        }
    }
    assert!(runs >= 8, "{runs} runs");
}
