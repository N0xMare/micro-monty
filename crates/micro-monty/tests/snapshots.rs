//! FULL snapshots (docs/spec.md §9.3): a session saved at any stop (a park
//! at any depth, a host call, a wait on host futures, between cells) and
//! restored continues exactly as the original would, with the same call
//! ids; a restored session saves to the same bytes as the original;
//! malformed snapshots are typed errors, never panics.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    Engine, EngineConfig, HostArgs, HostClass, HostModule, HostValue, Limits, Progress, Resume,
    Session, SessionSettings, SessionTemplate, SnapshotError, Stream, TextCut, Unconverted,
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
    hopping_in(&engine(), src, name, fuel, every)
}

/// A fuel park must not restart the scheduler's periodic check. Both
/// runs cross a collection between gathers that span several checks.
#[test]
fn scheduler_turns_continue_across_fuel_parks_and_restore() {
    let e = engine();
    let code = e
        .compile(
            "\
import asyncio, gc
async def child(i):
    await asyncio.sleep(0)
    return i
async def main():
    a = await asyncio.gather(*[child(i) for i in range(301)])
    gc.collect()
    b = await asyncio.gather(*[child(i) for i in range(317)])
    return sum(a) + sum(b)
asyncio.run(main())",
            "turns.py",
        )
        .expect("compiles");
    let run = |fuel| {
        let mut s = Session::new(&e).expect("session");
        s.set_fuel(fuel);
        let mut p = s.start(&code);
        let mut parks = 0;
        while let Progress::Parked { .. } = p {
            s = e
                .restore(&s.snapshot().expect("park image"))
                .expect("restore");
            s.set_fuel(fuel);
            p = s.resume(Resume::Continue).expect("continue");
            parks += 1;
        }
        assert!(
            matches!(&p, Progress::Complete(Some(v)) if v.repr == "95236"),
            "{p:?}"
        );
        assert_eq!(parks > 0, fuel.is_some());
        s.set_fuel(None);
        s.snapshot().expect("end state")
    };
    assert!(run(None) == run(Some(101)));
}

/// [`hopping`] in engine `e`.
fn hopping_in(
    e: &Engine,
    src: &str,
    name: &str,
    fuel: u64,
    every: usize,
) -> (String, String, usize) {
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = Session::new(e).expect("session");
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

/// A `**` spread that did not fit runs again after a collection, then its
/// call (`Vm::oom_rerun_at` names the instruction that is the rerun). A
/// restore at every park writes images while that mark is set, naming the
/// spread or the `CALL_EX`: a restore that dropped it took the call's
/// failure for the rerun's and raised (`['ok', 'MemoryError', 'ok',
/// 'MemoryError']` [M, reviewer]). python3.14 prints four oks.
#[test]
fn a_spread_rerun_mark_survives_a_restore() {
    let src = "\
N = 200_000
base = {'k%d' % i: i for i in range(N)}
def f(**kw):
    return len(kw)
out = []
for i in range(4):
    try:
        out.append('ok' if f(**base) == N else 'bad')
    except MemoryError:
        out.append('MemoryError')
print(out, len(base))
";
    let e = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(48 << 20)));
    let (restored, _, parks) = hopping_in(&e, src, "rerun.py", 10_007, 1);
    assert!(parks > 1, "{parks} parks");
    assert_eq!(restored, "['ok', 'ok', 'ok', 'ok'] 200000\n");
}

/// Callbacks registered on pending loop futures grow the run queue
/// ahead of their settle (`Vm::queue_spare`); a restore that did not give
/// the queue that room back left a restored session less of it than the
/// one that went on, so after the program filled memory the settles raised
/// `MemoryError` (after 71 futures at 3.2 MiB [M, reviewer]) where the
/// unparked run settles all 2,000 and runs their 20,000 callbacks. A
/// restore at every park must print the same.
#[test]
fn a_restored_run_queue_keeps_the_room_of_loop_future_callbacks() {
    let src = "\
import asyncio
await asyncio.sleep(0)
loop = asyncio.get_running_loop()
ran = 0
def cb(f):
    global ran
    ran += 1
fs = [loop.create_future() for _ in range(2000)]
for f in fs:
    for _ in range(10):
        f.add_done_callback(cb)
pad = []
try:
    while True:
        pad.append(bytearray(4096))
except MemoryError:
    del pad[-4:]
settled = 0
try:
    for f in fs:
        f.set_result(1)
        settled += 1
except MemoryError:
    pass
await asyncio.sleep(0)
pad = None
print(settled, ran)
";
    let e = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(3_355_443)));
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = Session::new(&e).expect("session");
    attach(&mut s, &out);
    let p = s.start(&e.compile(src, "hint.py").expect("compiles"));
    let want = (out.lock().expect("output lock").clone(), outcome(&p));
    assert_eq!(want.0, "2000 20000\n", "unparked");
    for slice in [147_000, 167_000] {
        let (text, end, parks) = hopping_in(&e, src, "hint.py", slice, 1);
        assert!(parks > 0, "{slice}: {parks} parks");
        assert_eq!((text, end), want, "restored at every park of {slice}");
    }
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
/// fuel (the third, only without a snapshot). In the first two, a later
/// operation that runs out of memory at once is not retried, as unparked:
/// the restored count is the waited collection's, not the one kept
/// through it.
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
parts = ['x' * 1000] * 30000
gc.collect()
t = [0] * 300000
t = None
try:
    OP
    st = 'ok'
except MemoryError:
    st = 'ME'
print(st, type(x).__name__, x is y, list.__len__(x), c)
try:
    z = ''.join(parts)
except MemoryError:
    pass
";
    // `f(*big, **kw)` (`CALL_EX`) builds its 1.6 MiB `*args` tuple before
    // the copy of `kw` fails, so the rerun allocates over 1 MiB before it
    // fails again: a rerun not marked as one retried again, after every
    // restore (the reviewer's guard check [M]), and at slices of 50,003
    // after every park, with no snapshot [M]. Capturing and restoring
    // preserve the retry and must cost no additional guest fuel.
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
    for (src, op, want) in [
        (
            format!(
                "{head}pad = bytearray(6000000)\nx = list(range(900000))\ny = x\nrhs = T(src)\n{}",
                tail.replace("OP", "x += rhs")
            ),
            "x += rhs",
            "ME list True 900000 {'i': 0, 'li': 0}\n",
        ),
        (
            format!(
                "{head}pad = bytearray(12000000)\nx = L(range(300000))\ny = x\nrhs = Idx()\n{}",
                tail.replace("OP", "x *= rhs")
            ),
            "x *= rhs",
            "ME L True 300000 {'i': 1, 'li': 0}\n",
        ),
        (callex.to_owned(), "f(*big, **kw)", "ME\n"),
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
                assert_eq!(charged, fuel, "{op} {m:?}");
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
/// preserves the mark and the restored session continues it. Tiny slices
/// from any heap size make that happen in a small one,
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

/// A restore empties the call caches, and the call that missed one put
/// the callee's frame above all of the caller's registers, where a hit
/// runs it in place: each restore in a recursion moved every deeper frame
/// up, so the stack grew past a doubling the session that went on never
/// needed. At 4 MiB beside a 3,800 KiB pad, `f(900)` printed `ok` going
/// on and `ME` restored at every park of 7 units [M].
#[test]
fn a_restore_does_not_move_the_frames_of_cached_calls() {
    let args = vec!["n"; 200].join(", ");
    let src = format!(
        "pad = bytes(3800 * 1024)\ndef h(*a):\n    return 0\ndef f(n):\n    if n:\n        r = f(n - 1)\n    else:\n        r = 0\n    t = h({args})\n    return r\ntry:\n    f(900)\n    print('ok')\nexcept MemoryError:\n    print('ME')\n"
    );
    let e = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(4 << 20)));
    let run = |restore: bool| {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = Session::new(&e).expect("session");
        attach(&mut s, &out);
        s.set_fuel(Some(7));
        let mut p = s.start(&e.compile(&src, "frames.py").expect("compiles"));
        while let Progress::Parked { .. } = p {
            if restore {
                s = e
                    .restore(&s.snapshot().expect("snapshot"))
                    .expect("restore");
                attach(&mut s, &out);
            }
            s.set_fuel(Some(7));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        out.lock().expect("output lock").clone()
    };
    assert_eq!(run(false), "ok\n");
    assert_eq!(run(true), "ok\n");
}

/// A cell near its memory limit whose outcome and fuel follow its garbage:
/// a snapshot used to collect first, so a snapshot at every third park of
/// 20,011 units moved its fuel (184,013 to 177,834 units in a 2 MiB
/// session) where parks alone did not [M].
const NEAR_LIMIT: &str = "\
def load(k, sz):
    return [bytearray(sz) for _ in range(k)]
def gen(k, sz):
    n = len(load(k, sz))
    i = 0
    while i < 300:
        i += 1
    yield n
    yield len(load(1, sz))
log = []
H = {}
try:
    H['g1'] = gen(4, 419430)
    print(next(H['g1']))
except MemoryError:
    log.append(1)
try:
    H['g2'] = gen(1, 1000000)
    print(next(H['g2']))
except MemoryError:
    log.append(2)
for k in list(H):
    del H[k]
try:
    H['d4'] = {j: str(j) for j in range(4194)}
    print(len(H['d4']))
except MemoryError:
    log.append(4)
try:
    H['st5'] = set(range(16666))
    H['st5'] = H['st5'] | set(range(16666, 33333))
except MemoryError:
    log.append(5)
try:
    print(len(load(1, 100000)))
except MemoryError:
    log.append(6)
print('cell', len(log), log[-3:])
";

/// Tasks parked beside the running one, whose buffers a restore charges.
const PARKED_TASKS: &str = "\
import asyncio
async def worker(k):
    acc = []
    for i in range(400):
        acc.append([i, str(i) * k])
        if i % 7 == 0:
            await asyncio.sleep(0)
    return len(acc)
async def main():
    return await asyncio.gather(*[worker(k) for k in range(4)])
print(asyncio.run(main()))
";

/// Runs `src` in a `mib` MiB session parked every `fuel` units, with a
/// snapshot at every park if `snap` (each restored, and the restored
/// session's image required to be the one it came from); the output,
/// outcome, fuel spent and the image at the end.
fn parked(
    src: &str,
    mib: u64,
    incremental: bool,
    fuel: u64,
    snap: bool,
) -> (String, String, u64, Vec<u8>) {
    let e = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(mib << 20)));
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = Session::new(&e).expect("session");
    attach(&mut s, &out);
    if incremental {
        s.set_gc_incremental(64, 0);
    }
    s.set_fuel_total(Some(u64::MAX));
    s.set_fuel(Some(fuel));
    let mut p = s.start(&e.compile(src, "exact.py").expect("compiles"));
    while let Progress::Parked { .. } = p {
        if snap {
            let bytes = s.snapshot().expect("snapshot");
            let mut r = e.restore(&bytes).expect("restore");
            assert!(
                r.snapshot().expect("snapshot") == bytes,
                "a restored session saves other bytes"
            );
        }
        s.set_fuel(Some(fuel));
        p = s.resume(Resume::Continue).expect("resumes");
    }
    let spent = u64::MAX - s.fuel_total().expect("fuel total");
    s.set_fuel_total(None);
    let text = out.lock().expect("output lock").clone();
    (text, outcome(&p), spent, s.snapshot().expect("snapshot"))
}

/// A snapshot changes nothing in the session it is taken from (docs/spec.md
/// §9.3): with one at every park, under default and incremental marks, the
/// output, outcome, fuel and final image are those of the same parks
/// without snapshots, and each image restores into a session that saves
/// it back unchanged.
#[test]
fn a_snapshot_changes_nothing_in_the_session_it_is_taken_from() {
    for (src, mib, fuel) in [
        (NEAR_LIMIT, 2, 20_011),
        (NEAR_LIMIT, 2, 997),
        (PARKED_TASKS, 16, 97),
    ] {
        for incremental in [false, true] {
            let want = parked(src, mib, incremental, fuel, false);
            let got = parked(src, mib, incremental, fuel, true);
            let what = format!("{mib} MiB, parks every {fuel}, incremental {incremental}");
            assert_eq!(
                (&got.0, &got.1, got.2),
                (&want.0, &want.1, want.2),
                "{what}"
            );
            assert!(got.3 == want.3, "{what}: the final images differ");
        }
    }
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

/// What the programs of [`a_restore_inside_a_handler_keeps_the_handled_exception`]
/// read the handled exception with: `sys.exception()`, a bare `raise` and
/// the `__context__` of the next exception raised.
const SEE: &str = "\
import sys
def see(tag):
    out = [tag, repr(sys.exception())]
    try:
        raise
    except RuntimeError as e:
        out.append('none' if 'No active' in str(e) else 're ' + repr(e))
    except BaseException as e:
        out.append('re ' + repr(e))
    try:
        raise OSError('probe')
    except OSError as e:
        out.append(repr(e.__context__))
    print(*out)
def under(exc, f, *a):
    if exc is None:
        return f(*a)
    try:
        raise exc
    except type(exc):
        return f(*a)
";

/// The handled exception of a frame suspended inside a handler survives a
/// restore at every park, at each kind of suspension: a generator at its
/// `yield`, a `yield from` chain, coroutines a `send()` suspended and
/// asyncio tasks (with an async generator). Each program resumes its
/// frames under callers that handle other exceptions, or none, so a
/// handler that began under one resumption ends under another; what each
/// prints is python3.14's output [M], unparked and restored at every park.
#[test]
#[allow(clippy::too_many_lines, reason = "four programs as source text")]
fn a_restore_inside_a_handler_keeps_the_handled_exception() {
    let cases: [(&str, &str, &str); 4] = [
        // A generator.
        (
            "handled_gen.py",
            "\
def g():
    try:
        raise KeyError('own')
    except KeyError:
        yield 1
        see('in handler')
        yield 2
    see('after handler')
    try:
        raise IndexError('second')
    except IndexError:
        try:
            raise ZeroDivisionError('nested')
        except ZeroDivisionError:
            yield 3
        see('after nested')
    see('end')
    yield 4
it = g()
print(under(ValueError('a'), next, it))
print(under(None, next, it))
print(under(TypeError('c'), next, it))
print(under(LookupError('d'), next, it))
see('module')
same = ValueError('same')
def h():
    try:
        yield 0
    except ValueError:
        yield 1
        see('same object')
    see('after same')
    yield 2
it = h()
next(it)
under(same, it.throw, same)
next(it)
",
            "\
1
in handler KeyError('own') re KeyError('own') KeyError('own')
2
after handler TypeError('c') re TypeError('c') TypeError('c')
3
after nested IndexError('second') re IndexError('second') IndexError('second')
end LookupError('d') re LookupError('d') LookupError('d')
4
module None none None
same object ValueError('same') re ValueError('same') ValueError('same')
after same None none None
",
        ),
        // A `yield from` chain.
        (
            "handled_yf.py",
            "\
def leaf():
    try:
        raise KeyError('leaf')
    except KeyError:
        yield 'l1'
        see('leaf in handler')
    see('leaf after')
    yield 'l2'
def mid():
    try:
        raise IndexError('mid')
    except IndexError:
        yield from leaf()
        see('mid in handler')
    see('mid after')
    yield 'm'
def top():
    yield from mid()
    see('top after')
it = top()
print(under(ValueError('y1'), next, it))
print(under(None, next, it))
print(under(TypeError('y3'), next, it))
print(under(LookupError('y4'), next, it, 'end'))
see('module')
",
            "\
l1
leaf in handler KeyError('leaf') re KeyError('leaf') KeyError('leaf')
leaf after IndexError('mid') re IndexError('mid') IndexError('mid')
l2
mid in handler IndexError('mid') re IndexError('mid') IndexError('mid')
mid after TypeError('y3') re TypeError('y3') TypeError('y3')
m
top after LookupError('y4') re LookupError('y4') LookupError('y4')
end
module None none None
",
        ),
        // A `send()` chain.
        (
            "handled_send.py",
            "\
class Aw:
    def __await__(self):
        return (yield 'aw')
async def inner():
    try:
        raise KeyError('inner')
    except KeyError:
        await Aw()
        see('inner in handler')
    see('inner after')
    await Aw()
    see('inner end')
async def outer():
    try:
        raise IndexError('outer')
    except IndexError:
        await inner()
        see('outer in handler')
    see('outer after')
    await Aw()
    see('outer end')
c = outer()
for exc in [ValueError('s1'), None, TypeError('s3'), LookupError('s4')]:
    try:
        print(under(exc, c.send, None))
    except StopIteration:
        print('returned')
    see('caller')
",
            "\
aw
caller None none None
inner in handler KeyError('inner') re KeyError('inner') KeyError('inner')
inner after IndexError('outer') re IndexError('outer') IndexError('outer')
aw
caller None none None
inner end IndexError('outer') re IndexError('outer') IndexError('outer')
outer in handler IndexError('outer') re IndexError('outer') IndexError('outer')
outer after TypeError('s3') re TypeError('s3') TypeError('s3')
aw
caller None none None
outer end LookupError('s4') re LookupError('s4') LookupError('s4')
returned
caller None none None
",
        ),
        // Asyncio tasks and an async generator.
        (
            "handled_task.py",
            "\
import asyncio
async def agen():
    try:
        raise KeyError('agen')
    except KeyError:
        yield 1
        see('agen in handler')
        await asyncio.sleep(0)
    see('agen after')
    yield 2
async def worker(n):
    try:
        raise KeyError(n)
    except KeyError:
        await asyncio.sleep(0)
        see('worker %d in handler' % n)
        try:
            raise IndexError(n)
        except IndexError:
            await asyncio.sleep(0)
            see('worker %d nested' % n)
        see('worker %d after nested' % n)
    await asyncio.sleep(0)
    see('worker %d after' % n)
async def main():
    it = agen()
    try:
        raise ValueError('m1')
    except ValueError:
        await anext(it)
        t = asyncio.create_task(worker(1))
        await asyncio.sleep(0)
        see('main in handler')
    await anext(it)
    await asyncio.gather(t, worker(2))
    see('main end')
asyncio.run(main())
see('module')
",
            "\
main in handler ValueError('m1') re ValueError('m1') ValueError('m1')
agen in handler KeyError('agen') re KeyError('agen') KeyError('agen')
worker 1 in handler KeyError(1) re KeyError(1) KeyError(1)
agen after None none None
worker 1 nested IndexError(1) re IndexError(1) IndexError(1)
worker 1 after nested KeyError(1) re KeyError(1) KeyError(1)
worker 1 after None none None
worker 2 in handler KeyError(2) re KeyError(2) KeyError(2)
worker 2 nested IndexError(2) re IndexError(2) IndexError(2)
worker 2 after nested KeyError(2) re KeyError(2) KeyError(2)
worker 2 after None none None
main end None none None
module None none None
",
        ),
    ];
    for (name, body, want) in cases {
        let src = format!("{SEE}{body}");
        let (plain_out, plain_outcome) = plain(&src, name);
        assert_eq!(plain_out, want, "{name}, unparked");
        for fuel in [1, 7] {
            let (got, got_outcome, parks) = hopping(&src, name, fuel, 1);
            assert!(parks > 10, "{name}, fuel {fuel}: {parks} parks");
            assert_eq!(got, want, "{name}, fuel {fuel}");
            assert_eq!(got_outcome, plain_outcome, "{name}, fuel {fuel}");
        }
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
    let restored = Vm::read_snapshot(
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
bad = []
for k in ('/etc/passwd', '../etc/passwd', 'Europe//Paris', 'Europe/./Paris', ''):
    try:
        ZoneInfo(k)
    except ValueError as e:
        bad.append(type(e).__name__)
print(bad)
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
        "2:00:00 True\n['ValueError', 'ValueError', 'ValueError', 'ValueError', 'ValueError']\nTrue CET\n2:00:00\nZoneInfoNotFoundError 'No time zone found with key \
         Asia/Tokyo (this sandbox has no time zone data: the host provides none; \
         datetime.timezone works for fixed offsets)'\n0:00:00\n"
    );
}

/// A host future that only a done callback waits on is pending, as one a
/// task awaits is: `shield`, `wait` (with and without a timeout),
/// `wait_for` over `shield` and an `Event` set from `add_done_callback`
/// wait for the host to answer, as `CPython` waits, instead of raising
/// "every task is waiting" or timing out at once. Each shape runs plain
/// and with a snapshot and restore at every stop, with the same output.
#[test]
fn host_futures_waited_on_through_callbacks_are_pending() {
    let e = engine();
    let shapes = [
        ("print(await asyncio.shield(fut(1)))", "10\n"),
        (
            "done, pend = await asyncio.wait([fut(3), fut(4)])\n    \
             print(sorted(f.result() for f in done), len(pend))",
            "[30, 40] 0\n",
        ),
        (
            "done, pend = await asyncio.wait([fut(3), fut(4)], timeout=5)\n    \
             print(sorted(f.result() for f in done), len(pend))",
            "[30, 40] 0\n",
        ),
        (
            "print(await asyncio.wait_for(asyncio.shield(fut(13)), 5))",
            "130\n",
        ),
        (
            "ev = asyncio.Event()\n    f = fut(2)\n    \
             f.add_done_callback(lambda _: ev.set())\n    await ev.wait()\n    print(f.result())",
            "20\n",
        ),
    ];
    for (body, want) in shapes {
        let src = format!("import asyncio\nasync def main():\n    {body}\nasyncio.run(main())\n");
        let code = e.compile(&src, "futs.py").expect("compiles");
        for restore in [false, true] {
            let out = Arc::new(Mutex::new(String::new()));
            let mut s = Session::new(&e).expect("session");
            s.define(&HostModule::untyped("", &["fut"]))
                .expect("defines");
            attach(&mut s, &out);
            let mut answers = Vec::new();
            let mut p = s.start(&code);
            loop {
                if restore && !matches!(p, Progress::Complete(_) | Progress::Error(_)) {
                    s = e
                        .restore(&s.snapshot().expect("snapshot"))
                        .expect("restores");
                    attach(&mut s, &out);
                }
                let resume = match &p {
                    Progress::HostCall { call, args, .. } => {
                        let HostValue::Int(n) = args[0] else {
                            panic!("{args:?}")
                        };
                        answers.push((*call, Ok(HostValue::Int(n * 10))));
                        Resume::Future
                    }
                    Progress::Waiting { calls } => Resume::Batch(
                        answers
                            .iter()
                            .filter(|(c, _)| calls.contains(c))
                            .cloned()
                            .collect(),
                    ),
                    _ => break,
                };
                p = s.resume(resume).expect("resumes");
            }
            assert_eq!(
                outcome(&p),
                "complete Some(\"None\")",
                "{body} (restore {restore})"
            );
            assert_eq!(
                *out.lock().expect("output lock"),
                want,
                "{body} (restore {restore})"
            );
        }
    }
}

#[allow(
    dead_code,
    reason = "the image helpers only; tests/fuzzed_images.rs replays the seeds"
)]
#[path = "../../../fuzz/fuzz_targets/snapshot_restore_input.rs"]
mod fuzz_input;

/// The header's stop must be what the VM is stopped at: real images at
/// each of the four phases, the header rewritten to each other phase (the
/// checksum recomputed), are `Corrupt`; rewritten to their own phase, they
/// restore and finish. A host call under another call id is corrupt too.
/// The waiting image holds a future the cell cancelled, whose answer
/// `resume` accepts and drops.
#[test]
fn a_stop_the_vm_is_not_at_is_corrupt() {
    let e = engine();
    let image = |src: &str, fuel: Option<u64>, futures: bool| {
        let mut s = Session::new(&e).expect("session");
        s.define(&HostModule::untyped("", &["ask"]))
            .expect("defines");
        s.set_fuel(fuel);
        let mut p = s.start(&e.compile(src, "stop.py").expect("compiles"));
        while futures && matches!(p, Progress::HostCall { .. }) {
            p = s.resume(Resume::Future).expect("future");
        }
        (s.snapshot().expect("snapshot"), p)
    };
    let waiting = "import asyncio\nasync def main():\n    f = asyncio.ensure_future(ask(1))\n\
                   \x20   await asyncio.sleep(0)\n    f.cancel()\n    return await ask(2)\n\
                   asyncio.run(main())\n";
    let images = [
        image("x = 1\n", None, false),
        image("y = ask(1) + 1\ny\n", None, false),
        image(waiting, None, true),
        image(
            "t = 0\nfor i in range(10_000):\n    t += i\nt\n",
            Some(50),
            false,
        ),
    ];
    for (phase, (img, stop)) in (0u8..).zip(&images) {
        assert_eq!(fuzz_input::stop_phase(img), Some(phase), "{stop:?}");
        for to in 0..4u8 {
            let mut forged = fuzz_input::forge_stop(img, to).expect("a stop");
            fuzz_input::reseal(&mut forged);
            let restored = e.restore(&forged);
            if to != phase {
                assert_eq!(
                    restored.err(),
                    Some(SnapshotError::Corrupt {
                        what: "stop".into()
                    }),
                    "phase {phase} forged to {to}"
                );
                continue;
            }
            let mut s = restored.expect("restores");
            assert_eq!(s.pending().as_ref(), (phase != 0).then_some(stop));
            let answer = match stop {
                Progress::HostCall { .. } => Resume::Value(HostValue::Int(2)),
                Progress::Waiting { calls } => {
                    assert_eq!(calls, &[2]);
                    Resume::Batch(vec![(1, Ok(HostValue::Int(0))), (2, Ok(HostValue::Int(5)))])
                }
                Progress::Parked { .. } => {
                    s.set_fuel(None);
                    Resume::Continue
                }
                _ => continue,
            };
            let p = s.resume(answer).expect("resumes");
            let want = ["", "Some(\"3\")", "Some(\"5\")", "Some(\"49995000\")"][usize::from(phase)];
            assert_eq!(outcome(&p), format!("complete {want}"));
        }
    }
    let mut other = images[1].0.clone();
    other[45] ^= 0x40;
    fuzz_input::reseal(&mut other);
    assert_eq!(
        e.restore(&other).err(),
        Some(SnapshotError::Corrupt {
            what: "stop".into()
        })
    );
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
    let template = SessionTemplate::capture(&engine, &mut restored).expect("captures");
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

/// The programs `forged_json_and_unicodedata_step_states_are_errors`
/// forges: setup, statement, and the op with its value slots and raw
/// words (the op's included), as in `nstep::STEPS`.
fn forged_step_cases() -> Vec<(&'static str, &'static str, (u8, usize, usize))> {
    const SCAN: (u8, usize, usize) = (15, 5, 16);
    let mut cases = vec![
        // A strip building its bitmap over steps (`STR_SCAN`).
        (
            "c = 'é𐐷' * 2_000_000 + 'a'\ns = 'é' * 2_000 + 'x'",
            "x = s.strip(c)",
            (11, 4, 8),
        ),
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
    // A `struct` field copied a stride job at a time (`BYTES`), which the
    // engine's own tests reach only through a bytes slice.
    if cfg!(feature = "mod-struct") {
        cases.push((
            "import struct\nb = b'ab' * 1_048_576",
            "x = struct.pack('2097152s', b)\ny = struct.unpack('2097152s', x)",
            (14, 4, 26),
        ));
    }
    if cfg!(feature = "mod-unicodedata") {
        cases.push((
            "import unicodedata\ns = 'e\\u0301\\xe9' * 150_000",
            "x = unicodedata.normalize('NFC', s)",
            (17, 3, 10),
        ));
    }
    cases
}

/// A forged image cannot crash or hang json's and unicodedata's steps
/// (`JSON_SCAN`, `JSON_DUMP`, `UNI_NORM`), a strip's or a `struct`
/// field's: each step checks every word its
/// state saved against the data before using it, so a word no step leaves
/// is the usual `SystemError`, or at worst a different result. Every word
/// of the state is forged at a few parks inside each kind of step. The
/// review of eb78933 aborted the process with a `JSON_SCAN` position of
/// `u64::MAX` (a slice past the input); a fill position inside an escape,
/// or its end past the input, looped forever in one step.
#[test]
fn forged_json_and_unicodedata_step_states_are_errors() {
    let eng = engine();
    let cases = forged_step_cases();
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

/// Runs `cells` (split at `#---`) in one session with host function `fut`,
/// each call answered with its argument (`Resume::Future`, then a batch of
/// every call a `Waiting` lists), under a fuel total that a cell polling
/// for an answer it is never given exhausts. With `restore`, a snapshot and
/// restore at every stop. Its stdout, and whether any cell reported
/// `Waiting`.
fn run_futs(e: &Engine, cells: &str, restore: bool) -> (String, bool) {
    let out = Arc::new(Mutex::new(String::new()));
    let attach_stdout = |s: &mut Session| {
        let sink = Arc::clone(&out);
        s.set_output_streams(Box::new(move |stream, text: &str| {
            if stream != Stream::Stderr {
                sink.lock().expect("output lock").push_str(text);
            }
        }));
    };
    let mut s = Session::new(e).expect("session");
    s.define(&HostModule::untyped("", &["fut"]))
        .expect("defines");
    attach_stdout(&mut s);
    s.set_fuel_total(Some(5_000_000));
    let mut waited = false;
    for (n, cell) in cells.split("#---\n").enumerate() {
        let code = e.compile(cell, "cell.py").expect("compiles");
        let mut answers = Vec::new();
        let mut p = s.start(&code);
        loop {
            if restore && !matches!(p, Progress::Complete(_) | Progress::Error(_)) {
                s = e
                    .restore(&s.snapshot().expect("snapshot"))
                    .expect("restores");
                attach_stdout(&mut s);
            }
            let resume = match &p {
                Progress::HostCall { call, args, .. } => {
                    answers.push((*call, Ok(args[0].clone())));
                    Resume::Future
                }
                Progress::Waiting { calls } => {
                    waited = true;
                    Resume::Batch(
                        answers
                            .iter()
                            .filter(|(c, _)| calls.contains(c))
                            .cloned()
                            .collect(),
                    )
                }
                Progress::Complete(_) | Progress::Error(_) => break,
                other => panic!("cell {n}: {other:?} (restore {restore}):\n{cells}"),
            };
            p = s.resume(resume).expect("resumes");
        }
        if let Progress::Error(err) = &p {
            panic!("cell {n} (restore {restore}):\n{cells}\n{}", err.traceback);
        }
    }
    let text = out.lock().expect("output lock").clone();
    (text, waited)
}

/// A sleep waits for the cell's pending host calls, as a deadline does
/// (host calls take no virtual time), and every unsettled host future of
/// the cell is pending, awaited or not. So a loop polling with
/// `sleep(t)` gets its answer, and a sleep or deadline beside a host call
/// fires after it, as on `CPython` (each expected output is python3.14's,
/// with `fut` a loop future answered soon after). Before, the polls ran
/// until the fuel was gone and the sleeps fired first. A host future from
/// an earlier cell is not asked for again: awaiting it raises the
/// scheduler's `RuntimeError` instead of a `Waiting` the host cannot
/// answer, and the session goes on (`CPython` answers it: host calls do not
/// outlive their cell). A future nothing needs is not waited for when the
/// cell ends, nor by `sleep(0)`.
#[test]
fn sleeps_wait_for_pending_host_calls() {
    let e = engine();
    let task =
        "import asyncio\nasync def w(f): return await f\nt = asyncio.create_task(w(fut(19)))\n";
    let callback = "import asyncio\ngot = []\nf = fut(19)\nf.add_done_callback(lambda f: got.append(f.result()))\n";
    let deadline = "try:\n    await asyncio.wait_for(asyncio.sleep(3), 1)\n    r = 'slept'\n\
                    except TimeoutError:\n    r = 'timeout'\n";
    let shapes: Vec<(String, &str, bool)> = vec![
        // Polling a task under `asyncio.timeout`, and without it.
        (
            format!("{task}async with asyncio.timeout(5):\n    while not t.done():\n        await asyncio.sleep(0.001)\nprint(t.result())\n"),
            "19\n",
            true,
        ),
        (
            format!("{task}n = 0\nwhile not t.done() and n < 5000:\n    await asyncio.sleep(0.001)\n    n += 1\nprint(t.done() and t.result())\n"),
            "19\n",
            true,
        ),
        // Polling for a done callback, with and without the timeout.
        (
            format!("{callback}async with asyncio.timeout(5):\n    while not got:\n        await asyncio.sleep(0.001)\nprint(got)\n"),
            "[19]\n",
            true,
        ),
        (
            format!("{callback}n = 0\nwhile not got and n < 2000:\n    await asyncio.sleep(0.001)\n    n += 1\nprint(got)\n"),
            "[19]\n",
            true,
        ),
        // A deadline beside a done callback, and beside a task.
        (format!("{callback}{deadline}print((r, got))\n"), "('timeout', [19])\n", true),
        (format!("{task}{deadline}print((r, t.done()))\n"), "('timeout', True)\n", true),
        // Sleeps of 2 s and 3 s and a 1 s deadline beside a task.
        (
            format!("{task}order = []\nasync def s(d):\n    await asyncio.sleep(d)\n    order.append(d)\n\
                     async def dl():\n    try:\n        await asyncio.wait_for(asyncio.sleep(10), 1)\n    \
                     except TimeoutError:\n        order.append('deadline')\n\
                     await asyncio.gather(s(2), s(3), dl())\nprint(order, await t)\n"),
            "['deadline', 2, 3] 19\n",
            true,
        ),
        // A future nothing waits on: polled, and beside a deadline.
        (
            "import asyncio\nf = fut(1)\nwhile not f.done():\n    await asyncio.sleep(0.001)\nprint(f.result())\n".to_owned(),
            "1\n",
            true,
        ),
        (format!("import asyncio\nf = fut(1)\n{deadline}print(f.done())\n"), "True\n", true),
        (
            "import asyncio\nf = fut(5)\nawait asyncio.sleep(1)\nprint('end', f.done())\n".to_owned(),
            "end True\n",
            true,
        ),
        // Not waited for: the cell ends, or only yields.
        ("f = fut(5)\nprint('end')\n".to_owned(), "end\n", false),
        (
            "import asyncio\nasync def w(f): return await f\nt = asyncio.create_task(w(fut(5)))\n\
             await asyncio.sleep(0)\nprint('end', t.done())\n"
                .to_owned(),
            "end False\n",
            false,
        ),
        (
            "import asyncio\nf = fut(5)\nawait asyncio.sleep(0)\nprint('end', f.done())\n".to_owned(),
            "end False\n",
            false,
        ),
        // A future of an earlier cell.
        (
            "f8 = fut(8)\n#---\nimport asyncio\ntry:\n    print(await f8)\nexcept RuntimeError as e:\n    \
             print('RuntimeError', e)\n#---\nprint('next cell')\n"
                .to_owned(),
            "RuntimeError every task is waiting and no host call is pending\nnext cell\n",
            false,
        ),
        (
            "f8 = fut(8)\n#---\nimport asyncio\ntry:\n    print(await asyncio.gather(f8, fut(9)))\n\
             except RuntimeError as e:\n    print('RuntimeError', e)\n"
                .to_owned(),
            "RuntimeError every task is waiting and no host call is pending\n",
            true,
        ),
    ];
    for (cells, want, waits) in &shapes {
        for restore in [false, true] {
            let (got, waited) = run_futs(&e, cells, restore);
            assert_eq!(got, *want, "restore {restore}:\n{cells}");
            assert_eq!(
                waited, *waits,
                "Waiting reported, restore {restore}:\n{cells}"
            );
        }
    }
}

/// Where a cell's regular fuel checks fall does not depend on restores
/// or slices (`Vm::regular_check`): cells holding several host futures,
/// and native work forcing checks, run plain, with a snapshot and restore
/// at every stop, and in fuel slices, spend the same fuel and end with the
/// same fuel left. A restore used to start `burned` at 0, so the check it
/// forces fell elsewhere and the fuel left at the end differed (12
/// futures: 99,786 plain, 99,807 restored).
#[test]
fn fuel_checks_fall_where_they_would_unparked() {
    let e = engine();
    let bodies = [
        "print(await asyncio.gather(fut(1), fut(2)))",
        "fs = [fut(i) for i in range(6)]\n    print([await f for f in fs])",
        "fs = [fut(i) for i in range(12)]\n    print(await asyncio.gather(*fs))",
        "s = 0\n    for i in range(3000):\n        s += len(str(i) * 40)\n    print(s, await fut(1))",
    ];
    for body in bodies {
        let src = format!("import asyncio\nasync def main():\n    {body}\nasyncio.run(main())\n");
        let code = e.compile(&src, "futs.py").expect("compiles");
        let mut seen = Vec::new();
        for (restore, slice) in [
            (false, None),
            (true, None),
            (false, Some(37)),
            (true, Some(211)),
        ] {
            let out = Arc::new(Mutex::new(String::new()));
            let mut s = Session::new(&e).expect("session");
            s.define(&HostModule::untyped("", &["fut"]))
                .expect("defines");
            attach(&mut s, &out);
            s.set_fuel_total(Some(10_000_000));
            s.set_fuel(Some(slice.unwrap_or(100_000)));
            let mut answers = Vec::new();
            let mut p = s.start(&code);
            loop {
                if restore && !matches!(p, Progress::Complete(_) | Progress::Error(_)) {
                    s = e
                        .restore(&s.snapshot().expect("snapshot"))
                        .expect("restores");
                    attach(&mut s, &out);
                }
                if let Some(n) = slice {
                    s.set_fuel(Some(n));
                }
                let resume = match &p {
                    Progress::HostCall { call, args, .. } => {
                        let HostValue::Int(n) = args[0] else {
                            panic!("{args:?}")
                        };
                        answers.push((*call, Ok(HostValue::Int(n * 10))));
                        Resume::Future
                    }
                    Progress::Waiting { calls } => Resume::Batch(
                        answers
                            .iter()
                            .filter(|(c, _)| calls.contains(c))
                            .cloned()
                            .collect(),
                    ),
                    Progress::Parked { .. } => Resume::Continue,
                    _ => break,
                };
                p = s.resume(resume).expect("resumes");
            }
            assert_eq!(outcome(&p), "complete Some(\"None\")", "{body}");
            let spent = 10_000_000 - s.fuel_total().expect("a total");
            let left = slice.is_none().then(|| s.fuel());
            seen.push((restore, slice, spent, left));
        }
        let (_, _, spent, left) = seen[0];
        for &(restore, slice, s2, l2) in &seen[1..] {
            assert_eq!(
                s2, spent,
                "{body}: fuel spent, restore {restore}, slice {slice:?}"
            );
            if let Some(l2) = l2 {
                assert_eq!(Some(l2), left, "{body}: fuel left, restore {restore}");
            }
        }
    }
}

/// A `call_soon` whose start cannot fit: a method of a Python function with
/// 100,000 arguments, whose start builds the ~800 KB tuple of them, beside
/// about 300 KB free (memory sized by a probe at 64 KiB, so the margins are
/// far above the accounting's own drift). `garbage` leaves 2 MB the start's
/// collection frees, so the held start succeeds; without it the retry fails
/// too and `MemoryError` is raised at the cell's `await`. The arguments go
/// on the handle, not through `call_soon(o.m, *args)`: a spread's register
/// peak stays in the VM stack's capacity, which a restore trims, so a run
/// restored only inside the hold would have that much more room.
fn held_start_cell(garbage: bool) -> String {
    format!(
        "\
import asyncio
import gc
class O:
    def m(self, *a):
        print('ran', len(a))
o = O()
junk = bytearray(2_000_000) if {garbage} else None
loop = asyncio.get_running_loop()
h = loop.call_soon(o.m)
h._args = tuple(range(100_000))
lo, hi = 0, 16 << 20
while hi - lo > 65536:
    mid = (lo + hi) // 2
    try:
        b = bytearray(mid)
        lo = mid
    except MemoryError:
        hi = mid
    b = None
gc.collect()
pad = bytearray(lo - 300_000)
junk = None
print('armed')
try:
    await asyncio.sleep(0)
    print('await returned')
except MemoryError:
    print('MemoryError at the await')
pad = None
gc.collect()
await asyncio.sleep(0)
print('end')
",
        garbage = if garbage { "True" } else { "False" }
    )
}

fn held_start_engine() -> Engine {
    Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(8 << 20)))
}

const HELD_TOTAL: u64 = 1 << 40;

fn with_sink(s: &mut Session) -> Arc<Mutex<String>> {
    let out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&out);
    s.set_output(Box::new(move |t: &str| {
        sink.lock().expect("lock").push_str(t);
    }));
    out
}

/// A snapshot taken while the scheduler holds a failed start for its
/// collection (`LOOP_START_WAIT`) and restored in a new process: the held
/// start then succeeds, or fails again and raises at the cell's `await`,
/// each with the output and fuel of the run that never parked.
#[test]
fn a_start_held_for_its_collection_restores_in_a_new_process() {
    let e = held_start_engine();
    for garbage in [true, false] {
        let code = e
            .compile(&held_start_cell(garbage), "held.py")
            .expect("compiles");
        let mut s = Session::new(&e).expect("session");
        let out = with_sink(&mut s);
        s.set_fuel_total(Some(HELD_TOTAL));
        let p = s.start(&code);
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        let plain = (
            out.lock().expect("lock").clone(),
            HELD_TOTAL - s.fuel_total().expect("total"),
        );
        let want = if garbage {
            "armed\nran 100000\nawait returned\nend\n"
        } else {
            "armed\nMemoryError at the await\nran 100000\nend\n"
        };
        assert_eq!(plain.0, want);

        // Parked every 997 units; the park inside the hold is the first
        // after `armed` whose slice paid a collection (`collect_for_retry`).
        let mut s = Session::new(&e).expect("session");
        let out = with_sink(&mut s);
        s.set_fuel_total(Some(HELD_TOTAL));
        s.set_fuel(Some(997));
        let mut p = s.start(&code);
        let mut last = HELD_TOTAL;
        let image = loop {
            assert!(
                matches!(p, Progress::Parked { .. }),
                "garbage {garbage}: no hold: {p:?}"
            );
            let left = s.fuel_total().expect("total");
            if out.lock().expect("lock").contains("armed") && last - left > 50_000 {
                break s.snapshot().expect("snapshot");
            }
            last = left;
            s.set_fuel(Some(997));
            p = s.resume(Resume::Continue).expect("resumes");
        };
        let before = out.lock().expect("lock").clone();
        assert_eq!(before, "armed\n");
        let path =
            std::env::temp_dir().join(format!("mm-held-{}-{garbage}.img", std::process::id()));
        std::fs::write(&path, &image).expect("write");
        let child = std::process::Command::new(std::env::current_exe().expect("this test"))
            .args(["held_start_in_a_child", "--exact", "--nocapture"])
            .env("MM_HELD_IMAGE", &path)
            .output()
            .expect("runs");
        let _ = std::fs::remove_file(&path);
        assert!(child.status.success(), "{child:?}");
        let stdout = String::from_utf8_lossy(&child.stdout);
        let rest = stdout
            .split("held output: <<")
            .nth(1)
            .and_then(|t| t.split(">>").next())
            .expect("the child's output");
        let left: u64 = stdout
            .split("held fuel left: ")
            .nth(1)
            .and_then(|t| t.split_whitespace().next())
            .and_then(|t| t.parse().ok())
            .expect("the child's fuel");
        assert_eq!(
            (before + rest, HELD_TOTAL - left),
            plain,
            "garbage {garbage}"
        );
    }
}

/// The new process of the test above: restores `MM_HELD_IMAGE`, runs it to
/// its end in slices of 997 units and prints its output and fuel left.
#[test]
fn held_start_in_a_child() {
    let Some(path) = std::env::var_os("MM_HELD_IMAGE") else {
        return;
    };
    let e = held_start_engine();
    let mut s = e
        .restore(&std::fs::read(path).expect("read"))
        .expect("restores");
    let out = with_sink(&mut s);
    let mut p = Progress::Parked {
        reason: micro_monty::ParkReason::Fuel,
    };
    while matches!(p, Progress::Parked { .. }) {
        s.set_fuel(Some(997));
        p = s.resume(Resume::Continue).expect("resumes");
    }
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let text = out.lock().expect("lock").clone();
    println!("held output: <<{text}>>");
    println!("held fuel left: {}", s.fuel_total().expect("total"));
}

/// Live and cancelled `call_soon` handles interleaved, then a run of
/// cancelled ones, parked at every unit and restored from a new image at
/// every park: the parks that fall at a skipped handle hold
/// `LOOP_SKIP_PARK` in their image, and the run ends with the output and
/// fuel of the run that never parked.
#[test]
fn a_park_at_a_skipped_handle_restores() {
    let e = Engine::new(EngineConfig::default());
    let code = e
        .compile(
            "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()\n\
             ran = []\ndef cb(i):\n    ran.append(i)\n\
             for i in range(150):\n    h = loop.call_soon(cb, i)\n    if i % 3:\n        h.cancel()\n\
             for i in range(100):\n    loop.call_soon(cb, -1).cancel()\n\
             loop.call_soon(cb, 1000)\nawait asyncio.sleep(0)\nprint(len(ran), sum(ran))\n",
            "skips.py",
        )
        .expect("compiles");
    let mut s = Session::new(&e).expect("session");
    let out = with_sink(&mut s);
    s.set_fuel_total(Some(HELD_TOTAL));
    let p = s.start(&code);
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let plain = (
        out.lock().expect("lock").clone(),
        HELD_TOTAL - s.fuel_total().expect("total"),
    );
    assert_eq!(plain.0, "51 4675\n");

    let mut s = Session::new(&e).expect("session");
    let mut out = with_sink(&mut s);
    let mut text = String::new();
    s.set_fuel_total(Some(HELD_TOTAL));
    s.set_fuel(Some(1));
    let mut p = s.start(&code);
    let mut parks = 0;
    while matches!(p, Progress::Parked { .. }) {
        parks += 1;
        text.push_str(&core::mem::take(&mut *out.lock().expect("lock")));
        let image = s.snapshot().expect("snapshot");
        drop(s);
        s = e.restore(&image).expect("restores");
        out = with_sink(&mut s);
        s.set_fuel(Some(1));
        p = s.resume(Resume::Continue).expect("resumes");
    }
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    text.push_str(&out.lock().expect("lock"));
    assert!(parks > 250, "{parks}");
    assert_eq!((text, HELD_TOTAL - s.fuel_total().expect("total")), plain);
}

/// A restore decodes a bundled program's instructions only when one of its
/// codes first runs (`Vm::load_bytecode`). That must not show: the same
/// image restored on a fresh engine (instructions left out) and on an
/// engine whose module cache holds the stdlib modules prints, charges
/// memory and burns fuel alike after every cell, through decodes of
/// reached code (a suspended `heapq.merge` generator, `json`, `collections`,
/// prelude units) and a reclamation of dropped user code; and both print
/// and burn what the session that wrote the image does. (Their code
/// charge is the writer's: the image carries the room of each code's
/// tables.)
#[test]
fn a_restore_charges_the_same_whether_bundled_code_is_decoded() {
    use micro_monty::test_hooks::code_state;
    let setup = r"
import json, collections, heapq
class P:
    def __init__(self, v):
        self.v = v
it = heapq.merge([1, 4, 5], [2, 3])
first = next(it)
def f(x):
    return json.dumps(collections.Counter(x).most_common(2))
";
    let cells = [
        "f('abracadabra')",
        "list(it)",
        "def g():\n    return P(2)\ng().v",
        "del g\nimport re\nre.sub('a', 'b', 'banana')",
        "f('mississippi'), first",
    ];
    let e = engine();
    let mut original = Session::new(&e).expect("session");
    let p = original.start(&e.compile(setup, "s.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let image = original.snapshot().expect("snapshot");
    let fresh = engine();
    let mut lazy = fresh.restore(&image).expect("restore");
    let warm = engine();
    let mut warming = Session::new(&warm).expect("session");
    assert!(matches!(
        warming.start(&warm.compile(setup, "s.py").expect("compiles")),
        Progress::Complete(_)
    ));
    let mut decoded = warm.restore(&image).expect("restore");
    let (charge, _, left_out) = code_state(&lazy);
    // The warm engine holds the modules, not the prelude units the image
    // names: a few of their codes are still left out.
    assert!(left_out > 100, "{left_out} codes left out");
    assert!(code_state(&decoded).2 < left_out / 4);
    assert_eq!(code_state(&decoded).0, charge);
    assert_eq!(code_state(&decoded).1, code_state(&lazy).1);
    let mut sessions = [&mut original, &mut lazy, &mut decoded];
    for s in &mut sessions {
        s.set_fuel_total(Some(1 << 40));
    }
    for cell in cells {
        let runs: Vec<_> = sessions
            .iter_mut()
            .map(|s| {
                let code = s.compile(cell, "c.py").expect("compiles");
                let p = outcome(&s.start(&code));
                let (charge, available, _) = code_state(s);
                (p, s.fuel_total(), charge, available)
            })
            .collect();
        assert_eq!(runs[1], runs[2], "{cell}: restored, then decoded or not");
        assert_eq!(runs[0], runs[1], "{cell}: original and restored");
    }
    assert!(
        code_state(&lazy).2 > 0,
        "unreached bundled code stays as it was"
    );
    assert!(code_state(&lazy).2 < left_out, "reached code was decoded");
}

/// A restored session is charged what the session that wrote the image
/// is, and stays so through further loads: its code charge, the bytes its
/// limit has left and its next image are the original's after every cell.
/// The image carries the room of each code's tables (a load grows them as
/// the original's grows) and the code loaded since the last reclamation
/// (`Vm::prepare_cell`), so both reclaim the dropped functions at the same
/// cell: a restore that counted all its code as loaded since would reclaim
/// `dead` at its first cell, and its image would be the smaller one.
#[test]
fn a_restored_session_stays_charged_as_the_original_through_further_loads() {
    use micro_monty::test_hooks::code_state;
    let e = engine();
    let mut original = Session::new(&e).expect("session");
    let run = |s: &mut Session, src: &str| {
        let code = s.compile(src, "c.py").expect("compiles");
        let p = s.start(&code);
        assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
    };
    let defs = |tag: &str, n: usize| {
        (0..n).fold(String::new(), |mut out, i| {
            let _ = write!(
                out,
                "def {tag}{i}(x, y=1):\n    z = x + y * {i}\n    return [z, '{tag}{i}', (x, y)]\n"
            );
            out
        })
    };
    run(&mut original, &format!("import json\n{}", defs("kept", 40)));
    run(&mut original, "def dead(a):\n    return a + 1\ndel dead");
    let mut reclaimed_at = None;
    let mut sessions = Vec::new();
    for round in 0..12 {
        if round % 4 == 0 {
            // A template's trusted instantiation and a reset to it carry
            // the same count as the checked restore. A capture collects
            // its session, so the sessions that follow the original are
            // captured as it is, and their templates are the same.
            let template = SessionTemplate::capture(&e, &mut original).expect("captures");
            for (from, s) in sessions.iter_mut().enumerate() {
                let theirs = SessionTemplate::capture(&e, s).expect("captures");
                assert!(
                    theirs.image() == template.image(),
                    "round {round}, session {from}: templates differ"
                );
            }
            sessions.push(template.instantiate().expect("instantiates"));
            let mut reset = Session::new(&e).expect("session");
            reset.reset_to(&template).expect("resets to the template");
            sessions.push(reset);
        }
        let image = original.snapshot().expect("snapshot");
        sessions.push(e.restore(&image).expect("restore"));
        let before = code_state(&original).0;
        let cell = format!(
            "{}\nr{round} = json.dumps([t{round}0(1), kept{round}(2)])\n{}",
            defs(&format!("t{round}"), 30),
            (0..30).fold(String::new(), |mut out, i| {
                let _ = writeln!(out, "del t{round}{i}");
                out
            }),
        );
        run(&mut original, &cell);
        if code_state(&original).0 < before && reclaimed_at.is_none() {
            reclaimed_at = Some(round);
        }
        let now = (
            code_state(&original).0,
            code_state(&original).1,
            original.snapshot().expect("snapshot"),
        );
        for (from, s) in sessions.iter_mut().enumerate() {
            run(s, &cell);
            let (charge, available, _) = code_state(s);
            assert_eq!(
                (charge, available),
                (now.0, now.1),
                "round {round}, session {from}"
            );
            assert!(
                s.snapshot().expect("snapshot") == now.2,
                "round {round}, session {from}: images differ"
            );
        }
    }
    // Sessions restored before and after a reclamation both went on above.
    assert!(
        reclaimed_at.is_some_and(|r| (1..11).contains(&r)),
        "dropped code is reclaimed in a middle round: {reclaimed_at:?}"
    );
}

/// A template's trusted instantiation skips the checks that decode the
/// code of a suspended generator, but still decodes it: a collection reads
/// the instructions at the yield of a suspended generator it has not
/// cleared, before anything resumes it (debug builds assert they are
/// there). A snapshot that collects first writes them cleared; one that
/// does not leaves this the only decode. No module cache, so each
/// instantiation decodes for itself.
#[test]
fn a_trusted_restore_collects_around_a_suspended_bundled_generator() {
    let e = Engine::with_module_cache(EngineConfig::default(), 0);
    let mut s = Session::new(&e).expect("session");
    let setup = "import heapq\nit = heapq.merge([1, 4, 5], [2, 3], [0, 6])\nfirst = next(it)";
    let p = s.start(&e.compile(setup, "s.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let template = SessionTemplate::capture(&e, &mut s).expect("template");
    // Each instantiation decodes for itself.
    for _ in 0..2 {
        let mut t = template.instantiate().expect("instantiates");
        let cell = "import gc\ngc.collect()\n[first, *it]";
        let p = t.start(&e.compile(cell, "c.py").expect("compiles"));
        assert_eq!(outcome(&p), "complete Some(\"[0, 1, 2, 3, 4, 5, 6]\")");
    }
}

/// A loop callback enters `_aio_callback`'s frame at its `CALL_EX` without
/// the dispatch loop (`Vm::enter_callback` reads its instructions), so on a
/// restore that left the prelude unit's instructions out it decodes them
/// first: the callback runs as it does in the session that went on.
#[test]
fn a_restored_loop_runs_a_callback_whose_prelude_code_is_not_decoded() {
    let setup = r"
import asyncio
seen = []
async def main():
    asyncio.get_running_loop().call_soon(seen.append, 1)
    await asyncio.sleep(0)
asyncio.run(main())
";
    let cell = r"
def note(x):
    seen.append(x * 10)
async def again():
    asyncio.get_running_loop().call_soon(note, 2)
    await asyncio.sleep(0)
asyncio.run(again())
seen
";
    let e = engine();
    let mut s = Session::new(&e).expect("session");
    let p = s.start(&e.compile(setup, "s.py").expect("compiles"));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let image = s.snapshot().expect("snapshot");
    let fresh = engine();
    let mut restored = fresh.restore(&image).expect("restore");
    for s in [&mut s, &mut restored] {
        let code = s.compile(cell, "c.py").expect("compiles");
        let p = s.start(&code);
        assert_eq!(outcome(&p), "complete Some(\"[1, 20]\")");
    }
}

/// The setup a template is captured after, run again after `reset` in the
/// sessions [`Session::reset_to`] is compared with. Its tool calls and
/// tasks move the counters a reset keeps; it reads no entropy or clock.
const SETUP: &str = "\
from collections import namedtuple
import asyncio, os, sys
Point = namedtuple('Point', ['x', 'y'], defaults=[0])
class Tools:
    async def call(n):
        return tool(n)
async def warm():
    return [await t for t in [asyncio.ensure_future(Tools.call(i)) for i in range(2)]]
asyncio.run(warm())
del warm
def _count(n):
    i = 0
    while i < n:
        yield i
        i += 1
_ticks = _count(100)
next(_ticks)
sys.set_int_max_str_digits(5000)
_v = [sys.version_info, sys.flags, CONF, repr([1, [2]])]
CONF.append(1)
";

/// Cells that leave behind what a reset keeps counting or keeps: host call
/// ids, task numbers, the entropy source's position, a lowered recursion
/// limit, the safepoint phase, suspended generators holding dead data (in
/// the generator registry), and the setup's generator advanced. They load
/// nothing the setup did not.
const BEFORE: [&str; 2] = [
    "\
def held():
    n = len(bytearray(200_000))
    yield n
    yield 2
kept = [held() for _ in range(40)]
for k in kept:
    next(k)
next(_ticks)
async def main():
    return [await t for t in [asyncio.ensure_future(Tools.call(i)) for i in range(3)]]
print(asyncio.run(main()), os.urandom(2).hex())
for i in range(1000):
    pass
",
    "x = [Point(i) for i in range(7)]\nprint(tool(len(x)))\n",
];

/// Cells that load what the setup did not: bundled modules and prelude
/// parts, which a reset keeps (charged to the session) and a template
/// does not have.
const LOADING: &str = "\
import json, random, re, statistics
print(json.dumps({'a': [1, 2]}), re.sub('a+', '-', 'baaac'), statistics.mean([1, 2, 4]))
print(sorted({3, 1, 2}), dict.fromkeys('ab'), list(reversed(range(3))))
";

/// What a cell can see after the reset: call ids and task names, entropy,
/// the limits and the setup's suspended generator.
const AFTER: &str = "\
async def main():
    t = asyncio.ensure_future(Tools.call(5))
    return asyncio.current_task().get_name(), t.get_name(), await t
print(asyncio.run(main()), Point(1), tool('x'), os.urandom(2).hex(), sys.getrecursionlimit())
print(next(_ticks), next(_ticks), 'kept' in globals())
";

/// Globals, then how many 4 KB buffers fit before `MemoryError`: what is
/// charged to the session, with the fuel `run_cell` reports.
const FILL: &str = "\
print(sorted(k for k in globals() if not k.startswith('__')))
xs = []
try:
    while True:
        xs.append(bytearray(4000))
except MemoryError:
    pass
print(len(xs))
";

#[derive(Clone, Copy, Debug)]
enum Run {
    Plain,
    /// Parks every 997 fuel units and continues.
    Parked,
    /// Also snapshots at every park.
    Snapshotted,
    /// Also restores at every park, into a new session.
    Restored,
}

fn counting_settings(out: &Arc<Mutex<String>>) -> SessionSettings {
    let mut s = SessionSettings::default();
    let sink = Arc::clone(out);
    s.output = Some(Box::new(move |_, t: &str| {
        sink.lock().expect("output lock").push_str(t);
    }));
    s.wall_clock = Some(Box::new(|| 1_700_000_000.0));
    let mut counter = 0u8;
    s.entropy = Some(Box::new(move |b: &mut [u8]| {
        for byte in b {
            counter = counter.wrapping_add(37);
            *byte = counter;
        }
    }));
    s
}

/// Runs `src` the `run` way: its output, host calls with their ids, outcome
/// and the fuel it charged.
fn run_cell(s: &mut Session, e: &Engine, out: &Arc<Mutex<String>>, src: &str, run: Run) -> String {
    const SLICE: u64 = 997;
    let mut log = String::new();
    let sliced = !matches!(run, Run::Plain);
    s.set_fuel(Some(if sliced { SLICE } else { 1 << 40 }));
    let mut granted = if sliced { SLICE } else { 1 << 40 };
    let mut p = s.start(&e.compile(src, "cell.py").expect("compiles"));
    loop {
        p = match p {
            Progress::Parked { .. } => {
                if matches!(run, Run::Snapshotted | Run::Restored) {
                    let image = s.snapshot().expect("snapshot");
                    if matches!(run, Run::Restored) {
                        let park_settings = counting_settings(out);
                        // The entropy source continues as the host's would.
                        let mut moved = e.restore_with(&image, park_settings).expect("restores");
                        std::mem::swap(s, &mut moved);
                    }
                }
                s.set_fuel(Some(SLICE));
                granted += SLICE;
                s.resume(Resume::Continue).expect("resumes")
            }
            Progress::HostCall {
                call, name, args, ..
            } => {
                let _ = writeln!(log, "call {call} {name} {args:?}");
                let answer = args.first().cloned().unwrap_or(HostValue::None);
                s.resume(Resume::Value(answer)).expect("resumes")
            }
            done => {
                let used = granted - s.fuel().unwrap_or(0);
                let text = std::mem::take(&mut *out.lock().expect("output lock"));
                let _ = write!(log, "{text}{}\nfuel {used}\n", outcome(&done));
                return log;
            }
        };
    }
}

/// `reset_to` is a fresh session and the setup, with the counters, limits,
/// settings and hint overrides a reset keeps; it is reset and the setup
/// exactly when the cells before it loaded nothing the setup did not.
#[test]
fn reset_to_a_template_is_a_fresh_session_and_its_setup() {
    let engine =
        Engine::new(EngineConfig::default().with_limits(Limits::new().with_memory(3 << 20)));
    let mut tools = HostModule::untyped("", &["tool"]);
    tools.constants = vec![("CONF".to_owned(), HostValue::List(vec![HostValue::Int(1)]))];
    let setup = |s: &mut Session, out: &Arc<Mutex<String>>| {
        s.define(&tools).expect("defines");
        run_cell(s, &engine, out, SETUP, Run::Plain)
    };
    let session = |out: &Arc<Mutex<String>>| {
        let mut s = Session::new_with(&engine, counting_settings(out)).expect("session");
        s.set_hint(micro_monty::HintGroup::DataLibraries, Some("use a tool"))
            .expect("hint");
        s
    };
    let out = Arc::new(Mutex::new(String::new()));
    let mut capture = session(&out);
    setup(&mut capture, &out);
    // Captured after a reset that follows a cell: the template counts what
    // the setup moved, not the cell.
    run_cell(&mut capture, &engine, &out, BEFORE[1], Run::Plain);
    capture.reset().expect("resets");
    setup(&mut capture, &out);
    let template = SessionTemplate::capture(&engine, &mut capture).expect("captures");
    for run in [Run::Plain, Run::Parked, Run::Snapshotted, Run::Restored] {
        // As reset and the setup.
        let mut transcripts = Vec::new();
        for reset_to in [false, true] {
            let out = Arc::new(Mutex::new(String::new()));
            let mut s = session(&out);
            let mut log = setup(&mut s, &out);
            for cell in BEFORE {
                log += &run_cell(&mut s, &engine, &out, cell, run);
            }
            if reset_to {
                s.reset_to(&template).expect("resets to the template");
            } else {
                s.reset().expect("resets");
                setup(&mut s, &out);
            }
            // Saved between cells too, right after the reset.
            if matches!(run, Run::Snapshotted | Run::Restored) {
                let image = s.snapshot().expect("snapshot");
                if matches!(run, Run::Restored) {
                    s = Engine::new(EngineConfig::default().with_limits(engine.limits()))
                        .restore_with(&image, counting_settings(&out))
                        .expect("restores on a new engine");
                }
            }
            for cell in [AFTER, FILL, "import numpy"] {
                log += &run_cell(&mut s, &engine, &out, cell, run);
            }
            transcripts.push(log);
        }
        assert_eq!(transcripts[1], transcripts[0], "{run:?}");
        for shown in [
            "call 9 tool",
            "'Task-17', 'Task-18'",
            "1 2 False",
            "use a tool",
        ] {
            assert!(
                transcripts[0].contains(shown),
                "{shown}: {}",
                transcripts[0]
            );
        }
        // As a fresh session and the setup, after cells that loaded more.
        let fill = |s: &mut Session, out| run_cell(s, &engine, out, FILL, run);
        let out = Arc::new(Mutex::new(String::new()));
        let mut fresh = session(&out);
        setup(&mut fresh, &out);
        let expected = fill(&mut fresh, &out);
        let mut s = session(&out);
        setup(&mut s, &out);
        // The earlier template was captured after a reset, which keeps
        // the arena's layout. Compare a fresh setup to a fresh capture.
        let fresh_template = SessionTemplate::capture(&engine, &mut s).expect("captures");
        run_cell(&mut s, &engine, &out, LOADING, run);
        s.reset_to(&fresh_template).expect("resets to the template");
        assert_eq!(fill(&mut s, &out), expected, "{run:?}");
        // Where a reset keeps what the cells loaded.
        s.reset_to(&fresh_template).expect("resets to the template");
        run_cell(&mut s, &engine, &out, LOADING, run);
        s.reset().expect("resets");
        setup(&mut s, &out);
        assert_ne!(fill(&mut s, &out), expected, "{run:?}");
    }
    // Refused, the session unchanged, under other limits or host definitions.
    let out = Arc::new(Mutex::new(String::new()));
    let other =
        Engine::new(EngineConfig::default().with_limits(Limits::new().with_memory(4 << 20)));
    let mut s = Session::new_with(&other, counting_settings(&out)).expect("session");
    s.define(&tools).expect("defines");
    assert_eq!(s.reset_to(&template), Err(SnapshotError::Mismatch));
    let mut s = session(&out);
    s.define(&HostModule::untyped("", &["tool", "other"]))
        .expect("defines");
    run_cell(&mut s, &engine, &out, "x = 1", Run::Plain);
    assert_eq!(s.reset_to(&template), Err(SnapshotError::Mismatch));
    let log = run_cell(
        &mut s,
        &engine,
        &out,
        "print(x, callable(other))",
        Run::Plain,
    );
    assert!(log.starts_with("1 True\n"), "{log}");
}

/// `reset_to` keeps what the session's host set (output, wall clock,
/// entropy, hints, fuel, deadline, park handle, limits), with the template
/// captured under other ones, and takes the setup's recursion limit and
/// call log in the session's ids: as `reset` and the setup. It refuses,
/// the session unchanged, a template that would change function ids or
/// the local zone, or one of a stopped session.
#[test]
#[allow(clippy::too_many_lines, reason = "one scenario, step by step")]
fn reset_to_keeps_the_sessions_host_side_and_refuses_what_would_change_it() {
    use micro_monty::{HintGroup, HostArgs, ParkReason, SessionTemplate};
    const SETUP: &str = "import sys, os, time\nsys.setrecursionlimit(50)\nx = tool(7)\n";
    const CELL: &str = "\
import sys, os, time
from datetime import datetime
from zoneinfo import ZoneInfo
print(datetime(2026, 7, 1, tzinfo=ZoneInfo('Europe/Paris')).utcoffset())
def f(n):
    return 0 if n == 0 else 1 + f(n - 1)
try:
    f(100)
except RecursionError:
    print('deep')
print(sys.getrecursionlimit(), time.time(), os.urandom(3).hex(), tool(3))
tool(iter(()))
";
    let engine =
        Engine::new(EngineConfig::default().with_limits(Limits::new().with_memory(3 << 20)));
    let mut tools = HostModule::untyped("", &["tool"]);
    tools.constants = vec![("CONF".to_owned(), HostValue::List(vec![HostValue::Int(1)]))];
    // The template's host side differs from the sessions'.
    let theirs = Arc::new(Mutex::new(String::new()));
    let mut settings = SessionSettings::default();
    let sink = Arc::clone(&theirs);
    settings.output = Some(Box::new(move |_, t: &str| {
        sink.lock().expect("output").push_str(t);
    }));
    settings.wall_clock = Some(Box::new(|| 5.0));
    settings.entropy = Some(Box::new(|b: &mut [u8]| b.fill(0)));
    let mut capture = Session::new_with(&engine, settings).expect("session");
    capture
        .set_hint(HintGroup::DataLibraries, Some("the template's hint"))
        .expect("hint");
    capture.define(&tools).expect("defines");
    run_cell(&mut capture, &engine, &theirs, SETUP, Run::Plain);
    let template = SessionTemplate::capture(&engine, &mut capture).expect("captures");
    let session = |out: &Arc<Mutex<String>>| {
        let mut s = Session::new_with(&engine, counting_settings(out)).expect("session");
        s.set_hint(HintGroup::DataLibraries, Some("the session's hint"))
            .expect("hint");
        s.set_zone_source(Some(Box::new(|key: &str| {
            std::fs::read(format!("/usr/share/zoneinfo/{key}")).ok()
        })));
        s.set_host_args(HostArgs {
            lenient: true,
            ..HostArgs::default()
        });
        s.define(&tools).expect("defines");
        run_cell(&mut s, &engine, out, SETUP, Run::Plain);
        run_cell(
            &mut s,
            &engine,
            out,
            "sys.setrecursionlimit(900)\ntool(1), tool(2)",
            Run::Plain,
        );
        s
    };
    let mut transcripts = Vec::new();
    for reset_to in [false, true] {
        let out = Arc::new(Mutex::new(String::new()));
        let mut s = session(&out);
        s.set_fuel(Some(5_000));
        // Twice, the second time to a template captured right after the
        // first: its setup is what moved the counters since that reset.
        if reset_to {
            s.reset_to(&template).expect("resets to the template");
            let again = SessionTemplate::capture(&engine, &mut s).expect("captures");
            s.reset_to(&again).expect("resets to the template");
        } else {
            for _ in 0..2 {
                s.reset().expect("resets");
                run_cell(&mut s, &engine, &out, SETUP, Run::Plain);
            }
            s.set_fuel(Some(5_000));
        }
        let mut log = format!("fuel {:?}\n", s.fuel());
        for r in s.call_log() {
            let _ = writeln!(log, "logged {} {}", r.call, r.name);
        }
        log += &run_cell(&mut s, &engine, &out, CELL, Run::Plain);
        log += &run_cell(&mut s, &engine, &out, "import numpy", Run::Plain);
        // A bundled module this build leaves out is named as such.
        log += &run_cell(&mut s, &engine, &out, "import ast", Run::Plain);
        // Object addresses differ with the heap's layout.
        let mut parts = log.split(" at 0x");
        let mut log = parts.next().unwrap_or_default().to_owned();
        for part in parts {
            log += " at 0x";
            log += part.trim_start_matches(|c: char| c.is_ascii_hexdigit());
        }
        transcripts.push(log);
    }
    assert_eq!(transcripts[1], transcripts[0]);
    for shown in [
        "fuel Some(5000)",
        "deep\n50 1700000000.0",
        "the session's hint",
        "2:00:00",
        "Unconverted",
    ] {
        assert!(
            transcripts[0].contains(shown),
            "{shown}: {}",
            transcripts[0]
        );
    }
    // The setup's call, logged under the id it has after `reset` and the
    // setup: one past the last call before the reset.
    let last = |t: &str, p: &str| -> u64 {
        t.lines()
            .filter_map(|l| l.strip_prefix(p)?.split(' ').next()?.parse().ok())
            .next_back()
            .expect("a call")
    };
    assert_eq!(last(&transcripts[1], "logged "), 5, "{}", transcripts[1]);
    assert!(theirs.lock().expect("output").is_empty());

    // A deadline and a park request made before the reset still apply.
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&out);
    let park = s.park_handle();
    s.set_deadline(Some(std::time::Instant::now()));
    park.request();
    s.reset_to(&template).expect("resets to the template");
    let spin = engine
        .compile("while True: pass", "spin.py")
        .expect("compiles");
    let mut parks = Vec::new();
    for _ in 0..2 {
        s.set_fuel(Some(100_000));
        parks.push(s.start(&spin));
        s.cancel();
    }
    for reason in [ParkReason::Deadline, ParkReason::Requested] {
        assert!(parks.contains(&Progress::Parked { reason }), "{parks:?}");
    }

    // Refused, the session unchanged: function ids that would change (the
    // same module, its names numbered in another order), another zone, a
    // stopped session's image.
    let mut other = Session::new(&engine).expect("session");
    other
        .define(&HostModule::untyped("", &["other", "tool"]))
        .expect("defines");
    let numbered = SessionTemplate::capture(&engine, &mut other).expect("captures");
    let mut renumbered = Session::new_with(&engine, counting_settings(&out)).expect("session");
    renumbered.define(&tools).expect("defines");
    renumbered
        .define(&HostModule::untyped("", &["other", "tool"]))
        .expect("defines");
    let mut zoned = session(&out);
    zoned.set_local_timezone(Some(
        std::fs::read("/usr/share/zoneinfo/America/New_York").expect("zone"),
    ));
    let mut stopped = session(&out);
    stopped.set_fuel(Some(1 << 40));
    assert!(matches!(
        stopped.start(&engine.compile("tool(9)", "c.py").expect("compiles")),
        Progress::HostCall { .. }
    ));
    let busy = SessionTemplate::from_image(&engine, stopped.snapshot().expect("snapshot"))
        .expect("a template");
    stopped.cancel();
    for (s, template, error) in [
        (&mut renumbered, &numbered, SnapshotError::Mismatch),
        (&mut zoned, &template, SnapshotError::Mismatch),
        (&mut stopped, &busy, SnapshotError::Busy),
    ] {
        run_cell(s, &engine, &out, "kept = 1", Run::Plain);
        assert_eq!(s.reset_to(template), Err(error));
        let log = run_cell(s, &engine, &out, "print(kept)", Run::Plain);
        assert!(log.starts_with("1\n"), "{log}");
    }

    // A template read under the session's limits, not its engine's.
    let deep = Engine::new(
        EngineConfig::default().with_limits(
            Limits::new()
                .with_memory(64 << 20)
                .with_recursion_depth(5_000),
        ),
    );
    let mut wide = Session::new(&deep).expect("session");
    wide.define(&tools).expect("defines");
    let wide =
        SessionTemplate::from_image(&deep, wide.snapshot().expect("snapshot")).expect("a template");
    let mut s = Session::new_with(&engine, counting_settings(&out)).expect("session");
    s.define(&tools).expect("defines");
    s.reset_to(&wide).expect("resets to the template");
    let log = run_cell(
        &mut s,
        &engine,
        &out,
        "import sys\ndef f(n):\n    return 0 if n == 0 else 1 + f(n - 1)\ntry:\n    f(2_000)\nexcept RecursionError:\n    print('deep', sys.getrecursionlimit())",
        Run::Plain,
    );
    assert!(log.starts_with("deep 1000\n"), "{log}");
    let log = run_cell(
        &mut s,
        &engine,
        &out,
        "sys.setrecursionlimit(4_000)\ntry:\n    f(2_000)\nexcept RecursionError:\n    print('capped')",
        Run::Plain,
    );
    assert!(log.starts_with("capped\n"), "{log}");
    let log = run_cell(
        &mut s,
        &engine,
        &out,
        "try:\n    b = bytearray(8_000_000)\n    print('room')\nexcept MemoryError:\n    print('MemoryError')",
        Run::Plain,
    );
    assert!(log.starts_with("MemoryError\n"), "{log}");

    // A stopped cell is dropped.
    let mut stopped = session(&out);
    stopped.set_fuel(Some(1 << 40));
    assert!(matches!(
        stopped.start(&engine.compile("tool(9)", "c.py").expect("compiles")),
        Progress::HostCall { .. }
    ));
    stopped.reset_to(&template).expect("resets to the template");
    assert_eq!(stopped.phase(), micro_monty::Phase::Idle);
    let log = run_cell(&mut stopped, &engine, &out, "print('idle')", Run::Plain);
    assert!(log.starts_with("idle\n"), "{log}");

    // Ids of functions the session defined before, which the template's
    // session never did, keep their numbers.
    let final_m = HostModule::untyped("m", &["f"]);
    let mut early = Session::new(&engine).expect("session");
    early.define(&final_m).expect("defines");
    let early = SessionTemplate::capture(&engine, &mut early).expect("captures");
    let mut s = Session::new(&engine).expect("session");
    s.define(&HostModule::untyped("m", &["f", "x"]))
        .expect("defines");
    s.define(&final_m).expect("defines");
    s.reset_to(&early).expect("resets to the template");
    assert_eq!(s.host_function_id("m.x"), Some(1));
}

/// Sets one GC setting on a session.
type Setting = fn(&mut Session);

/// The session's GC settings (testing modes) stay across `reset_to`: the
/// parks, marks and collections of a run are those of the template's
/// session instantiated with them (incremental marks pace by the heap,
/// which a restore lays out anew).
#[test]
fn reset_to_keeps_the_sessions_gc_settings() {
    let cell = |n: u32| {
        format!(
            "xs = []\nfor i in range({n}):\n    xs.append([i])\n    if len(xs) > 2000:\n        xs = []\n"
        )
    };
    let engine = Engine::new(EngineConfig::default());
    let run = |s: &mut Session, n: u32| {
        let before = s.gc_collections();
        s.set_fuel(Some(50));
        let mut p = s.start(&engine.compile(&cell(n), "c.py").expect("compiles"));
        let (mut parks, mut marking) = (0, 0);
        while let Progress::Parked { .. } = p {
            parks += 1;
            marking += usize::from(s.gc_marking());
            s.set_fuel(Some(50));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        (parks, marking, s.gc_collections() - before)
    };
    let template = SessionTemplate::capture(&engine, &mut Session::new(&engine).expect("session"))
        .expect("captures");
    let plain = |n| run(&mut Session::new(&engine).expect("session"), n);
    // The stress modes collect at every safepoint: a shorter run.
    let settings: [(Setting, u32); 5] = [
        (
            |s| {
                s.set_gc_incremental(16, 0);
                s.set_gc_yield(true);
            },
            40_000,
        ),
        (|s| s.set_gc_incremental(16, 0), 40_000),
        (|s| s.set_gc_stress(true), 2_000),
        (|s| s.set_gc_inc_stress(true), 2_000),
        (
            |s| {
                let mut off = SessionSettings::default();
                off.gc_off = true;
                *s =
                    Session::new_with(&Engine::new(EngineConfig::default()), off).expect("session");
            },
            40_000,
        ),
    ];
    let mut all = Vec::new();
    for (i, (set, n)) in settings.into_iter().enumerate() {
        let mut seen = Vec::new();
        for reset_to in [false, true] {
            let mut s = if reset_to {
                Session::new(&engine).expect("session")
            } else {
                template.instantiate().expect("instantiates")
            };
            set(&mut s);
            if reset_to {
                s.reset_to(&template).expect("resets to the template");
            }
            seen.push(run(&mut s, n));
        }
        assert_eq!(seen[1], seen[0], "setting {i}");
        assert_ne!(seen[0], plain(n), "setting {i} changes nothing");
        all.push(seen[0]);
    }
    // Yielding changes what incremental collection alone does.
    assert_ne!(all[0], all[1]);
}

/// Stops with a long opaque type name or registered host class name at
/// every list entry. Assert a full payload: a lenient stop marker would
/// otherwise make the image much smaller without failing the call.
fn named_host_stops(e: &Engine, opaque: usize, objects: usize) -> [(Progress, Vec<u8>); 2] {
    let opaque_name = "C".repeat(300);
    let class_name = "H".repeat(300);
    let qualified = format!("fs.{class_name}");
    [
        (
            format!("class {opaque_name}: pass\no = {opaque_name}()\ncheckpoint([o] * {opaque})"),
            opaque,
            true,
        ),
        (
            format!("import fs\no = {qualified}(1)\ncheckpoint([o] * {objects})"),
            objects,
            false,
        ),
    ]
    .map(|(src, copies, lenient)| {
        let mut s = Session::new(e).expect("session");
        s.define(&HostModule::untyped("", &["checkpoint"]))
            .expect("defines");
        s.set_host_args(HostArgs {
            lenient,
            ..HostArgs::default()
        });
        if !lenient {
            s.define(&HostModule {
                name: "fs".into(),
                classes: vec![HostClass {
                    name: class_name.clone(),
                    fields: vec![("x".into(), "int".into())],
                    ..HostClass::default()
                }],
                ..HostModule::default()
            })
            .expect("defines class");
        }
        let p = s.start(&e.compile(&src, "stop.py").expect("compiles"));
        let Progress::HostCall { args, .. } = &p else {
            panic!("expected a host call: {p:?}");
        };
        let [HostValue::List(values)] = args.as_slice() else {
            panic!("expected one list argument");
        };
        assert_eq!(values.len(), copies);
        assert!(values.iter().all(|v| match v {
            HostValue::Unconverted(Unconverted::Opaque { type_name, .. }) => {
                lenient && *type_name == opaque_name
            }
            HostValue::Object { class, .. } => !lenient && *class == qualified,
            _ => false,
        }));
        (p, s.snapshot().expect("snapshot with full payload"))
    })
}

/// The stop an image's header names (a host call's positional and
/// keyword argument values, a wait's calls) is charged to the restoring
/// engine's memory as it is read, each part before it is allocated: past
/// the limit the restore is `TooLarge`, never a header read as corrupt or
/// an allocation past the limit.
#[test]
fn a_stop_past_the_memory_limit_is_too_large() {
    let roomy = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(64 << 20)));
    let parked = |src: &str| {
        let mut s = Session::new(&roomy).expect("session");
        s.define(&HostModule::untyped("", &["checkpoint"]))
            .expect("defines");
        let p = s.start(&roomy.compile(src, "stop.py").expect("compiles"));
        assert!(matches!(p, Progress::HostCall { .. }), "{p:?}");
        s.snapshot().expect("snapshot")
    };
    let restore = |image: &[u8], mib: u64| {
        Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(mib << 20)))
            .restore(image)
            .map(drop)
            .err()
    };
    // One argument's text past the limit.
    let text = parked("checkpoint('x' * (3 << 20))");
    assert_eq!(restore(&text, 64), None);
    assert_eq!(restore(&text, 2), Some(SnapshotError::TooLarge));
    // Half a million `None` arguments: a byte each in the image, eight
    // in the restored heap's tuple, but a `HostValue` (56 bytes) each in
    // the stop, as the call charged them: on 24 MiB the heap fits and the
    // stop does not.
    let many = parked("checkpoint(*([None] * (1 << 19)))");
    assert_eq!(restore(&many, 64), None);
    assert_eq!(restore(&many, 24), Some(SnapshotError::TooLarge));
    // The encoded stop and heap fit in 8 MiB; the decoded stop's record
    // names and HostValue slots together do not.
    let records = parked(
        "from collections import namedtuple\nT = namedtuple('A' * 300, 'x')\ncheckpoint([T(1)] * 20_000)",
    );
    assert_eq!(restore(&records, 8), Some(SnapshotError::TooLarge));
    for (_, image) in named_host_stops(&roomy, 12_400, 20_000) {
        assert_eq!(restore(&image, 64), None);
        assert_eq!(restore(&image, 8), Some(SnapshotError::TooLarge));
    }
}

/// Both the live and the restored session convert the large record list
/// after resuming the empty checkpoint. Snapshot decoding of payloads
/// already built is covered by the stopped-call tests.
#[test]
fn record_names_count_towards_the_host_call_memory_limit() {
    let e = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(8 << 20)));
    for (copies, fits) in [(16_000, true), (20_000, false)] {
        let mut live = Session::new(&e).expect("session");
        live.define(&HostModule::untyped("", &["checkpoint"]))
            .expect("defines");
        live.set_fuel_total(Some(1_000_000));
        let src = format!(
            "from collections import namedtuple\nT = namedtuple('A' * 300, 'x')\n\
             checkpoint()\ncheckpoint([T(1)] * {copies})"
        );
        assert!(matches!(
            live.start(&e.compile(&src, "stop.py").expect("compiles")),
            Progress::HostCall { .. }
        ));
        let mut restored = e
            .restore(&live.snapshot().expect("snapshot"))
            .expect("restores");
        let p = live.resume(Resume::Value(HostValue::None)).expect("live");
        let q = restored
            .resume(Resume::Value(HostValue::None))
            .expect("restored");
        assert!(p == q, "{copies} copies: live and restored outcomes differ");
        assert_eq!(live.fuel_total(), restored.fuel_total());
        assert!(
            match p {
                Progress::HostCall { .. } => fits,
                Progress::Error(e) if e.type_name == "MemoryError" => !fits,
                _ => false,
            },
            "{copies} copies: expected the call to {}",
            if fits { "fit" } else { "exhaust memory" }
        );
    }
}

/// A host call's stop restores at its writer's limit: the restore charges
/// no more than the live call did, which releases each container's open
/// frame once it is built.
#[test]
fn a_stop_restores_at_its_writers_limit() {
    for (src, mib) in [
        // A container argument each.
        ("checkpoint(*([[]] * 100_000))", 16),
        // Long class names, one per record.
        (
            "from collections import namedtuple\nT = namedtuple('A' * 300, 'x')\ncheckpoint([T(1)] * 16_000)",
            8,
        ),
    ] {
        let e = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(mib << 20)));
        let mut s = Session::new(&e).expect("session");
        s.define(&HostModule::untyped("", &["checkpoint"]))
            .expect("defines");
        let p = s.start(&e.compile(src, "stop.py").expect("compiles"));
        assert!(matches!(p, Progress::HostCall { .. }), "{src}: {p:?}");
        let image = s.snapshot().expect("snapshot");
        assert_eq!(e.restore(&image).map(drop).err(), None, "{src}");
    }
    let e = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(8 << 20)));
    for (p, image) in named_host_stops(&e, 11_000, 17_000) {
        let restored = e.restore(&image).expect("restores at the writer's limit");
        assert!(restored.pending() == Some(p), "restored the full payload");
    }
}

/// A host module's constants are host definitions, whose names
/// `define_host_module` counts: a restore counts a record constant's class
/// name too, so the restored session saves the image it came from.
#[test]
fn a_record_constant_restores_to_the_same_image() {
    let e = engine();
    let mut s = Session::new(&e).expect("session");
    let record = HostValue::Record {
        name: "Point".to_owned(),
        fields: vec![("x".to_owned(), HostValue::Int(1))],
    };
    s.define(&HostModule {
        constants: vec![("ORIGIN".to_owned(), record)],
        ..HostModule::untyped("tools", &["get"])
    })
    .expect("defines");
    let image = s.snapshot().expect("snapshot");
    let mut restored = e.restore(&image).expect("restores");
    assert!(restored.snapshot().expect("snapshot") == image);
}

/// A budget set between cells is the next cell's in a session restored
/// from an image taken then, as in the session itself, which finishes
/// the cell where the engine's budget would not.
#[test]
fn a_restored_session_keeps_the_next_cells_fuel() {
    let e = Engine::new(EngineConfig::new().with_limits(Limits::new().with_fuel(Some(60))));
    let code = e
        .compile("n = 0\nfor i in range(100):\n    n += i\nn", "cell.py")
        .expect("compiles");
    for fuel in [Some(5_000), None] {
        let mut s = Session::new(&e).expect("session");
        s.set_fuel(fuel);
        let mut restored = e
            .restore(&s.snapshot().expect("snapshot"))
            .expect("restores");
        let ends = [&mut s, &mut restored].map(|s| {
            let p = s.start(&code);
            assert!(matches!(p, Progress::Complete(_)), "{fuel:?}: {p:?}");
            s.fuel()
        });
        assert_eq!(ends[0], ends[1], "{fuel:?}");
    }
}
