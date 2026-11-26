//! The step-time oracle: no native step runs longer than [`LIMIT`] between
//! two safepoints at the default limits (docs/spec.md §5, "Bounded native
//! steps"). Each row runs with a deadline already past, set again after
//! every park, so every safepoint that reads the clock parks; the longest
//! wall time between two parks is the row's longest step. Release builds
//! only: `cargo xtask check` builds it as shipped (std rebuilt,
//! `-Cpanic=immediate-abort`, `--cfg mm_shipped`) and runs it on one test
//! thread.
//!
//! Each row also runs on fuel slices of [`SLICE`] units, set again after
//! every fuel park: no slice may charge more than [`OVERDRAFT`] units past
//! what it was given (a chunked native's step and a little more), except
//! an operation that parked first with `fuel_wanted` and then got that
//! much. So a fuel budget overshoots by at most one step.
//!
//! The limit is 50 ms on the reference machine, scaled by this machine's
//! speed ([`speed`]): three probes, a fresh 64 MiB allocation filled once,
//! a fixed run of chunked steps and a chain of dependent loads over 64 MiB,
//! timed against the reference's, so a row's verdict does not depend on
//! which machine runs it (docs/spec.md §5.1, clause 2).
//!
//! A row is a site of guest-controlled work at about the largest size the
//! default 256 MiB allows. `Open` rows are known to exceed the limit and
//! are only reported, until their fix makes them `Bounded`; a `Bounded` row
//! over the limit runs again after the other rows ([`retry`]), and fails
//! the test, with its index, setup and program, if no run is under it.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{Engine, EngineConfig, ParkReason, Progress, Resume, Session};
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// The longest native step allowed on the reference machine: mm-wasm runs
/// about 1.5 to 3 times slower [E], so this keeps it under the wasm host's
/// epoch grace (100 ms plus 0.75 ms per MiB of the memory limit).
const LIMIT: Duration = Duration::from_millis(50);

/// The probes' times on the reference machine, an Amp orb (16 cores,
/// gVisor), release, 23c9646 [M]: least of 5, steady to ±1% over 3 rounds.
const REF_FRESH: Duration = Duration::from_micros(28_100);
const REF_SCAN: Duration = Duration::from_micros(11_600);
/// [`latency`]'s time on a reference-class orb with 144 ns random loads
/// [M]: rows bound by cache misses (a mark or a probe over a big table)
/// run up to twice as long on such an orb as on one with 64 ns loads.
const REF_LOADS: Duration = Duration::from_millis(144);

/// The slowest machine the oracle judges, as a ratio of the reference's
/// time: the limit stops scaling here, so a step over it on a slower
/// machine says nothing about the code.
const SLOWEST: f64 = 2.0;

/// How long the oracle waits in all for a machine past [`SLOWEST`] to come
/// back, probing again every [`REPROBE`]: before the first row runs and
/// before each retry. A gate's host can stay past it for a whole run, or
/// drift there after the limit was set (1.52 at the start, past 2 at the
/// retries) [S, the 0.0.3 release gates], so a machine still past it then
/// is refused as unable to judge, at once and with no row's verdict.
const PATIENCE: Duration = Duration::from_secs(300);
const REPROBE: Duration = Duration::from_secs(15);

/// How much slower than the reference this machine is: the largest ratio
/// of its probe times to [`REF_FRESH`], [`REF_SCAN`] and [`REF_LOADS`],
/// each the least of 5; the limits clamp it to 0.5..=[`SLOWEST`]
/// ([`settled`]). `fresh` allocates and fills
/// 64 MiB, the cost of a step that allocates an object at the cap; `scan`
/// runs four chunked steps of `STEP_WORK` units (`-1 in xs`), the cost of
/// native work; `loads` is memory latency ([`latency`]).
fn speed(engine: &Engine) -> (f64, Duration, Duration, Duration) {
    let mut fresh = Duration::MAX;
    for _ in 0..5 {
        let t = Instant::now();
        let mut v = vec![0u64; 1 << 23];
        v.fill(0xFFFB_0000_0000_0005);
        std::hint::black_box(&v);
        fresh = fresh.min(t.elapsed());
    }
    let mut scan = Duration::MAX;
    for _ in 0..5 {
        let mut s = Session::new(engine).expect("session");
        let setup = engine.compile("xs = list(range(131072))", "setup.py");
        let p = s.start(&setup.expect("setup compiles"));
        assert!(matches!(p, Progress::Complete(_)), "setup: {p:?}");
        let code = engine.compile(&"b = -1 in xs\n".repeat(4), "scan.py");
        let code = code.expect("compiles");
        let t = Instant::now();
        let p = s.start(&code);
        scan = scan.min(t.elapsed());
        assert!(matches!(p, Progress::Complete(_)), "scan: {p:?}");
    }
    let loads = latency();
    let ratio = (fresh.as_secs_f64() / REF_FRESH.as_secs_f64())
        .max(scan.as_secs_f64() / REF_SCAN.as_secs_f64())
        .max(loads.as_secs_f64() / REF_LOADS.as_secs_f64());
    (ratio, fresh, scan, loads)
}

/// The least of 5 times of a million dependent loads, each from a random
/// slot of a 64 MiB table (one cycle through all of it, so no load is
/// cached or prefetched): this machine's memory latency, a million times.
fn latency() -> Duration {
    let n = 1usize << 24;
    let mut next: Vec<u32> = (0..1 << 24).collect();
    // Sattolo's shuffle, from a fixed xorshift seed: one cycle.
    let mut x = 0x9E37_79B9_7F4A_7C15_u64;
    for i in (1..n).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        next.swap(i, usize::try_from(x % i as u64).unwrap_or(0));
    }
    let mut best = Duration::MAX;
    let mut p = 0u32;
    for _ in 0..5 {
        let t = Instant::now();
        for _ in 0..1_000_000 {
            p = next[p as usize];
        }
        best = best.min(t.elapsed());
    }
    std::hint::black_box(p);
    best
}

/// [`speed`], probed again every [`REPROBE`] while it is past [`SLOWEST`]
/// and `waited` is under [`PATIENCE`]; `waited` grows by the time spent.
/// The probe returned is the last, past [`SLOWEST`] only once patience is
/// spent.
fn settled(engine: &Engine, waited: &mut Duration) -> (f64, Duration, Duration, Duration) {
    loop {
        let t = Instant::now();
        let probe = speed(engine);
        if probe.0 <= SLOWEST || *waited >= PATIENCE {
            *waited += t.elapsed();
            return probe;
        }
        println!(
            "steps: this machine runs at {:.2} of the reference's time, past {SLOWEST}: \
             probing again in {REPROBE:?} ({waited:.0?} of {PATIENCE:?} waited)",
            probe.0
        );
        std::thread::sleep(REPROBE);
        *waited += t.elapsed();
    }
}

/// [`settled`], or the test fails at once if the machine is still past
/// [`SLOWEST`]: it cannot judge a step there.
fn judging(engine: &Engine, waited: &mut Duration) -> (f64, Duration, Duration, Duration) {
    let probe = settled(engine, waited);
    if probe.0 > SLOWEST {
        // A failing test aborts without its panic message (`cargo xtask
        // check` builds it with `-Zpanic-abort-tests`), so print it too.
        let refused = format!(
            "this machine takes {:.2} times the reference's time after {waited:.0?} of probing; \
             the step oracle cannot judge it",
            probe.0
        );
        println!("steps: {refused}");
        panic!("{refused}");
    }
    probe
}

/// Frees a chunk just under 32 MiB that glibc mmapped, which raises its
/// dynamic mmap threshold to its most: from then on it serves large
/// blocks from its heap, where growing one may copy it, as in any
/// long-lived host after its first large frees. Every row then sees the
/// same allocator, whatever ran before it.
fn raise_mmap_threshold() {
    let v = vec![1u8; (32 << 20) - (64 << 10)];
    drop(std::hint::black_box(v));
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Bounded,
    Open,
    /// `Bounded` on the libc's allocator, `Open` where the oracle runs on
    /// mimalloc (`steps_mimalloc.rs`): known to exceed the limit on some
    /// runs there (CHANGELOG, Known issues). Its fuel is judged either way.
    OpenOnMimalloc,
}

impl Row {
    /// How the timed oracle judges this row with this binary's allocator.
    fn timed(self) -> Row {
        // `steps_mimalloc.rs` includes this file as its module `rows`.
        let mimalloc = module_path!().starts_with("steps_mimalloc");
        match self {
            Row::OpenOnMimalloc if mimalloc => Row::Open,
            Row::OpenOnMimalloc => Row::Bounded,
            row => row,
        }
    }
}

/// The row that must park with a mark held: [`gaps`]' third count.
const HELD_MARK_ROW: &str = "k = d.items() - e";

/// The longest wall time between two safepoints while `src` runs after
/// `setup`, the number of parks, and how many of them held a mark.
fn gaps(engine: &Engine, setup: &str, src: &str) -> (Duration, usize, usize) {
    let mut s = Session::new(engine).expect("session");
    let p = s.start(&engine.compile(setup, "setup.py").expect("setup compiles"));
    assert!(matches!(p, Progress::Complete(_)), "setup: {p:?}");
    let code = engine.compile(src, "step.py").expect("compiles");
    let (mut worst, mut parks, mut marked) = (Duration::ZERO, 0, 0);
    let mut t = Instant::now();
    s.set_deadline(Some(t));
    let mut p = s.start(&code);
    loop {
        worst = worst.max(t.elapsed());
        if p != (Progress::Parked {
            reason: ParkReason::Deadline,
        }) {
            break;
        }
        parks += 1;
        marked += usize::from(s.gc_marking());
        t = Instant::now();
        s.set_deadline(Some(t));
        p = s.resume(Resume::Continue).expect("resumes");
    }
    assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
    (worst, parks, marked)
}

/// The most fuel one run between two safepoints may charge past the budget
/// it started with: one bounded step. A chunked native's step charges
/// about `STEP_WORK` (2^17 units); one unchunked operation over one object
/// at the 64 MiB cap charges up to about 2^20 for the bytes it reads (a
/// unit per 64) and as much for a result it builds, plus a step of the
/// code around it. An operation that would charge more chunks, or parks
/// first with `fuel_wanted`.
const OVERDRAFT: u64 = (1 << 21) + (1 << 17);

/// The fuel slice each park of [`overdraft`] gives.
const SLICE: u64 = 1_000;

/// The most fuel one slice charged past what it was given while `src` runs
/// after `setup`: `SLICE` units, or what `fuel_wanted` asked for after an
/// operation parked before starting.
fn overdraft(engine: &Engine, setup: &str, src: &str) -> u64 {
    let mut s = Session::new(engine).expect("session");
    let p = s.start(&engine.compile(setup, "setup.py").expect("setup compiles"));
    assert!(matches!(p, Progress::Complete(_)), "setup: {p:?}");
    let code = engine.compile(src, "step.py").expect("compiles");
    let total = u64::MAX / 2;
    s.set_fuel_total(Some(total));
    s.set_fuel(Some(SLICE));
    let mut left = total;
    let (mut worst, mut given) = (0, SLICE);
    let mut p = s.start(&code);
    loop {
        let now = s.fuel_total().expect("a total");
        worst = worst.max((left - now).saturating_sub(given));
        left = now;
        if p != (Progress::Parked {
            reason: ParkReason::Fuel,
        }) {
            break;
        }
        given = SLICE.max(s.fuel_wanted());
        s.set_fuel(Some(given));
        p = s.resume(Resume::Continue).expect("resumes");
    }
    assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
    worst
}

/// A set of 2^22 slots, 45% full, and `big`, 2M ints whose probes each
/// walk 16 full runs, every one free (the correctness review's AA4 rows).
const SET_FREE_RUNS: &str = "\
S = 1 << 22
mask = S - 1
qs = []
s = set()
r = 1
n = 0
while n < S * 0.45:
    q = (r * 0x9E3779B97F4A7C15) & ((1 << 60) - 1)
    r += 1
    i = q & mask
    p = q
    for _ in range(16):
        m = 10 if i + 9 <= mask else 1
        for j in range(m):
            s.add(i + j)
        p >>= 5
        i = (5 * i + 1 + p) & mask
    n += 160
    qs.append(q)
big = qs * (2_000_000 // len(qs) + 1)";

/// The 99 arguments after the yield in [`SETUP_W100`]'s call windows.
macro_rules! args99 {
    () => {
        concat!(
            "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, ",
            "18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, ",
            "33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, ",
            "48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, ",
            "63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, ",
            "78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, ",
            "93, 94, 95, 96, 97, 98, 99"
        )
    };
}

/// 32,000 generators of one function, every other one at its second
/// yield, each in a 100-register call window (`use(...)`'s arguments).
const SETUP_W100: &str = concat!(
    "def use(*a):\n",
    "    return len(a)\n",
    "def g(k):\n",
    "    n = 0\n",
    "    while True:\n",
    "        n += use((yield n), ",
    args99!(),
    ")\n",
    "        n += use(0, (yield n), ",
    args99!(),
    ")\n",
    "gens = [g(i) for i in range(32_000)]\n",
    "for i, x in enumerate(gens):\n",
    "    next(x)\n",
    "    if i % 2:\n",
    "        x.send(0)\n",
);

/// Names whose hashes put them in the first `$width` buckets of the intern
/// table they end in (`$mask` + 1 slots), which a guest picks by `hash()`:
/// `names`, `$n` of them, interned in bucket order, so they fill one run of
/// slots at little cost, and `probes`, 2,000 new names of that run's first
/// buckets, each of which walks all of it.
macro_rules! intern_cluster {
    ($n:literal, $mask:literal, $width:literal) => {
        concat!(
            "class O:\n",
            "    pass\n",
            "o = O()\n",
            "def pick(prefix, n, mask, width):\n",
            "    out = []\n",
            "    k = 0\n",
            "    while len(out) < n:\n",
            "        k += 1\n",
            "        s = prefix + str(k)\n",
            "        if hash(s) & mask < width:\n",
            "            out.append(s)\n",
            "    return out\n",
            "names = pick('q', ",
            $n,
            ", ",
            $mask,
            ", ",
            $width,
            ")\n",
            "names.sort(key=lambda s: hash(s) & ",
            $mask,
            ")\n",
            "for s in names:\n",
            "    hasattr(o, s)\n",
            "probes = pick('p', 2_000, ",
            $mask,
            ", 1024)\n",
        )
    };
}

/// 34,000 names of the first eighth of the buckets of a 131,072-slot
/// intern table, not yet interned, in no order: interned, they double the
/// table from 65,536 slots while they fill one run of its slots.
const INTERN_GROWS: &str = "\
class O:
    pass
o = O()
names = []
k = 0
while len(names) < 34_000:
    k += 1
    s = 'q' + str(k)
    if hash(s) & 131071 < 16384:
        names.append(s)";

/// [`intern_cluster!`]'s 72,000 names, and `text`: 20,000 of a Latin-1
/// char whose hash puts it among them.
const INTERN_CHAR: &str = concat!(
    intern_cluster!("72_000", "262143", "65536"),
    "vs = [c for c in range(128, 256) if hash(chr(c)) & 262143 < 8192]\n",
    "text = chr(vs[0]) * 20_000\n",
);

/// `crowd(h, n)`: `n` names whose hashes' low 18 bits lie 1 to 8 before
/// those of hash `h`, so that in any table of at most 262,144 slots they
/// fill the slots up to and past `h`'s; and `behind(names)`, which
/// interns them and has a collection lay them out, so that a string of
/// hash `h`, interned before or after, lies past them all: as far from
/// its slot as the intern table lets a string lie (`INTERN_WALK_CAP`).
macro_rules! intern_crowd {
    ($($rest:literal),*) => {
        concat!(
            "import gc\n",
            "class O:\n",
            "    pass\n",
            "o = O()\n",
            "def crowd(h, n):\n",
            "    out = []\n",
            "    k = 0\n",
            "    while len(out) < n:\n",
            "        k += 1\n",
            "        s = 'c' + str(k)\n",
            "        if 0 < (h - hash(s)) & 262143 <= 8:\n",
            "            out.append(s)\n",
            "    return out\n",
            "def behind(names):\n",
            "    for s in names:\n",
            "        hasattr(o, s)\n",
            "    names.pop()\n",
            "    s = None\n",
            "    gc.collect()\n",
            $($rest,)*
        )
    };
}

/// `names`: 160 of the first 128 buckets of every intern table.
/// Interned, the one that finds no slot within the cap grows the table
/// to the largest (4,194,304 slots), and a later one is refused there.
/// `$rest` runs after them, with `O` and `o`.
macro_rules! intern_crowd_every_table {
    ($($rest:literal),*) => {
        concat!(
            "class O:\n",
            "    pass\n",
            "o = O()\n",
            "names = []\n",
            "k = 0\n",
            "while len(names) < 160:\n",
            "    k += 1\n",
            "    s = 'c' + str(k)\n",
            "    if hash(s) & 4194303 < 128:\n",
            "        names.append(s)\n",
            $($rest,)*
        )
    };
}

const SET_METHOD_SETUP: &str =
    "s = set(range(750_000))\na = list(range(500_000))\nb = list(range(250_000, 750_000))";

/// (row, setup, the operation, times it repeats as straight-line code):
/// each at about the largest size the default limits allow. The phase 2
/// packages add their sites here.
const SITES: &[(Row, &str, &str, usize)] = &[
    // collect: an endless iterator, a chunk per step.
    (
        Row::Bounded,
        "import itertools",
        "xs = list(itertools.islice(enumerate(itertools.count()), 3_000_000))",
        1,
    ),
    // The pairs come a chunk per step, then the dict a slice of them per
    // step (`nstep::op::BULK`, P4); the collections during growth mark a
    // slice per step (28.5 ms [M]).
    (
        Row::Bounded,
        "it = iter(range(4_000_000))",
        "d = dict(zip(it, it))",
        1,
    ),
    // Natives that build tens of MiB of live data during a mark: the
    // remark traces it in one step, which used to share its gap with the
    // native's last chunk (53.8 ms and 49.3 ms on ca3db9e); a park now goes
    // ahead of a collection step (spec §9.2): 34 ms and 43 ms [M]. The
    // dict's mark is forced to finish in one step, and it gave up there for
    // a full collection (44-49 ms on 7df5282); a forced finish no longer
    // gives up, and finishes in 6 ms. The dict's worst gaps were then its
    // last insert slices at 155 MiB live (no GC step in them), bound by
    // memory latency: 17-22 ms on an orb with 64 ns random loads, 35-46 ms
    // on one with 144 ns [M]. An item into a table past 65,536 entries now
    // costs two units (set.rs `WIDE`): 11.4-12.6 ms on the first [M]. The dict's
    // zip is now read in place, its tuples at `TUPLE_ITEM` units each:
    // 9.9-10.9 ms on an orb at 0.77 of the reference [M]. Built by a
    // comprehension, `xs` leaves the zip a collection that falls due with
    // 9 MiB of room at 195 MiB live, which ran in full (44-54 ms on
    // f305b88); a mark with too little room is now held, a slice per step
    // with no guest code between (spec §9.2): 24-25 ms, the forced finish
    // of an earlier mark [M]. With 600k old lists the zip ends at 215 MiB,
    // where collections ran in full (72 ms); held, 23-24 ms [M].
    (
        Row::Bounded,
        "xs = [i for i in range(4_000_000)]\nold = [[i] for i in range(200_000)]",
        "ys = list(zip(xs, xs))",
        1,
    ),
    (
        Row::Bounded,
        "old = [[i] for i in range(600_000)]\nxs = list(range(1_000_000))\nss = [str(i) for i in range(1_000_000)]",
        "d = dict(zip(ss, xs))",
        1,
    ),
    // Sorts: arrays filled and merged a slice per step, keys a chunk per
    // step, the result reversed or mapped a slice per step.
    (
        Row::Bounded,
        "xs = [str((i * 7919) % 1_000_003) for i in range(1_000_000)]",
        "ys = sorted(xs)",
        1,
    ),
    (
        Row::Bounded,
        "xs = [(i * 7919) % 1_000_003 for i in range(2_000_000)]",
        "xs.sort(reverse=True)",
        1,
    ),
    // At 4M its longest gap was the merge arrays' allocation with the
    // collection it requested (28-46 ms; 54 ms with 64 MB more live). The
    // arrays now come unfilled, the second in a step of its own once the
    // runs are sorted: 36-39 -> 11-14 ms alone [M, the allocation floor].
    (
        Row::Bounded,
        "xs = [(i * 7919) % 1_000_003 for i in range(4_000_000)]",
        "xs.sort(reverse=True)",
        1,
    ),
    (
        Row::Bounded,
        "xs = [(i * 7919) % 1_000_003 for i in range(2_000_000)]",
        "ys = sorted(xs, key=abs, reverse=True)",
        1,
    ),
    (
        Row::Bounded,
        "xs = [((i * 7919) % 100, i) for i in range(1_000_000)]",
        "ys = sorted(xs)",
        1,
    ),
    // `in` over a list or tuple: `nstep::op::SEQ_IN`.
    (
        Row::Bounded,
        "xs = list(range(8_000_000))",
        "b = -1 in xs",
        1,
    ),
    (
        Row::Bounded,
        "t = tuple(range(5_000_000))",
        "b = -1 not in t",
        1,
    ),
    // Folds read a container's iterator in place, a step at a time
    // (`nstep::op::FOLD`); a tuple hashes a slice per call (`_thash`).
    (
        Row::Bounded,
        "xs = list(range(8_000_000))",
        "m = max(xs)",
        1,
    ),
    (
        Row::Bounded,
        "xs = [float(i) for i in range(4_000_000)]",
        "t = sum(xs)",
        1,
    ),
    (Row::Bounded, "s = 'a' * 8_000_000", "b = all(s)", 1),
    (
        Row::Bounded,
        "t = tuple(range(8_000_000))",
        "h = hash(t)",
        1,
    ),
    // Collecting a str, set or dict: a chunk a step, then `_collect_rest`
    // into a list sized up front. At 8M items, step 0 of `_collect_rest`
    // is the 64 MiB allocation (the mm-core floor, as for `[0] * 8M`).
    (Row::Bounded, "s = 'a' * 8_000_000", "xs = list(s)", 1),
    (
        Row::Bounded,
        "d = {i: i for i in range(2_000_000)}",
        "xs = list(d.items())",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(2_000_000))",
        "xs = sorted(a)",
        1,
    ),
    // The docs audit's fold and collect sources (P1's rows): str, bytes,
    // bytearray, set, frozenset, dict and its views, at about the largest
    // the default limits allow. On eb78933 the str and bytes folds raised
    // MemoryError (collect's 536 MB buffer) or ran 1.2-5.1 s in one step,
    // the set folds 135-174 ms, the dict ones 69-114 ms, and the collects
    // 70-191 ms [M, P1]. A set iterator's last `next` scanned an empty
    // tail slot by slot (56.6 ms at 2.6M items) until `Vm::set_next_at`.
    (Row::Bounded, "s = 'a' * 67_000_000", "x = min(s)", 1),
    (Row::Bounded, "s = 'é' * 33_000_000", "x = all(s)", 1),
    (Row::Bounded, "B = b'\\x01' * 67_108_864", "x = min(B)", 1),
    (Row::Bounded, "B = b'\\x01' * 67_108_864", "x = max(B)", 1),
    (Row::Bounded, "B = b'\\x01' * 67_108_864", "x = sum(B)", 1),
    (Row::Bounded, "B = b'\\x01' * 67_108_864", "x = all(B)", 1),
    (Row::Bounded, "B = b'\\x00' * 67_108_864", "x = any(B)", 1),
    (Row::Bounded, "B = bytearray(67_108_864)", "x = any(B)", 1),
    (Row::Bounded, "st = set(range(2_600_000))", "x = min(st)", 1),
    (
        Row::Bounded,
        "st = frozenset(range(2_600_000))",
        "x = sum(st)",
        1,
    ),
    (
        Row::Bounded,
        "st = set(range(1, 2_600_000))",
        "x = all(st)",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: 0 for i in range(2_000_000)}",
        "x = min(d)",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: 0 for i in range(2_000_000)}",
        "x = max(d.values())",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: 0 for i in range(2_000_000)}",
        "x = sum(d.values())",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: 0 for i in range(2_000_000)}",
        "x = any(d.values())",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: 0 for i in range(1, 2_000_000)}",
        "x = all(d.keys())",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: 0 for i in range(2_000_000)}",
        "x = min(d.items())",
        1,
    ),
    (Row::Bounded, "B = b'a' * 8_000_000", "xs = list(B)", 1),
    (Row::Bounded, "s = 'a' * 8_000_000", "xs = tuple(s)", 1),
    (
        Row::Bounded,
        "st = frozenset(range(2_600_000))",
        "xs = tuple(st)",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: 0 for i in range(2_000_000)}",
        "xs = list(d)",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: 0 for i in range(2_000_000)}",
        "xs = list(d.values())",
        1,
    ),
    // Growing a 2M-item set one `add` at a time: its table grows a slice
    // per step (`Vm::bulk_grow_top`).
    (
        Row::Bounded,
        "import copy\na = set(range(2_000_000))",
        "b = copy.deepcopy(a)",
        1,
    ),
    // `re`: a search runs one step slice per call, attempts (a lookahead
    // first) or the skip to a char a match can start with alike.
    (
        Row::Bounded,
        "import re\ns = 'a' * 60_000_000",
        "m = re.search('[xy]', s)",
        1,
    ),
    (
        Row::Bounded,
        "import re\ns = 'a' * 60_000_000",
        "m = re.search('(?=b)[bc]', s)",
        1,
    ),
    (
        Row::Bounded,
        "import re\ns = 'a' * 60_000_000",
        "n = len(re.findall(r'(DEBUG|INFO)\\d', s))",
        1,
    ),
    // A long container repr goes on in Python, which joins its pieces in
    // runs (`_repr_run`, so collections no longer mark 3M live pieces);
    // the final join goes a step at a time (`nstep::op::STR_JOIN`).
    (
        Row::Bounded,
        "xs = [[i] for i in range(1_000_000)]",
        "s = str(xs)",
        1,
    ),
    // Straight-line code (C4): a long operator after another stops for a
    // safepoint first (`Vm::stop_before`).
    (
        Row::Bounded,
        "s = 'a' * 60_000_000\nt = 'a' * 59_999_999 + 'b'",
        "b = s == t",
        64,
    ),
    // Single objects near the 64 MiB object cap (P1, P3, P5). `replace`:
    // counted, then built a slice per step (`nstep::op::STR_REPLACE`).
    (
        Row::Bounded,
        "s = 'ab' * 20_000_000",
        "t = s.replace('a', 'xy')",
        1,
    ),
    (
        Row::Bounded,
        "s = '1' * 30_000_000 + 'x'",
        "try:\n    int(s)\nexcept ValueError:\n    pass",
        1,
    ),
    // json (P5): a window of input a step (`nstep::op::JSON_SCAN`).
    (
        Row::Bounded,
        "import json\ns = json.dumps(list(range(5_000_000)))",
        "xs = json.loads(s)",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(2_000_000))\nb = set(range(1_000_000, 3_000_000))",
        "c = a & b",
        1,
    ),
    // str (P1): results written into their arena block a slice per step
    // (`nstep::op::STR_FILL` .. `STR_SCAN`, strmeth.rs).
    (Row::Bounded, "pass", "s = 'a' * 67_000_000", 1),
    (
        Row::Bounded,
        "xs = ['[123456]'] * 6_500_000",
        "s = ', '.join(xs)",
        1,
    ),
    (Row::Bounded, "s = 'a,' * 4_000_000", "xs = s.split(',')", 1),
    (
        Row::Bounded,
        "s = 'a,' * 4_000_000",
        "xs = s.rsplit(',')",
        1,
    ),
    (Row::Bounded, "s = 'abc ' * 2_000_000", "xs = s.split()", 1),
    (
        Row::Bounded,
        "s = 'a\\n' * 4_000_000",
        "xs = s.splitlines()",
        1,
    ),
    // Near the limit (6 MiB free), each of 61 parts of 70 KB asks for a
    // collection (`Vm::collect_for_result`), and each was topped up to the
    // retry's charge: one slice charged 32.6 million units past its 1,000
    // [M, reviewer]. Charged once per collection: 1.18 million [M].
    (
        Row::Bounded,
        "import gc\nbig = ('a' * 69999 + ',') * 60\npad = []\nmid = []\nsmall = []\n\
         for xs, n in ((pad, 1 << 20), (mid, 70001), (small, 2000)):\n    try:\n        \
         while True:\n            xs.append(bytes(n))\n    except MemoryError:\n        pass\n\
         del pad[-6:]\ngc.collect()",
        "parts = big.split(',')",
        1,
    ),
    (Row::Bounded, "s = 'é' * 33_000_000", "t = s.upper()", 1),
    (Row::Bounded, "s = 'a' * 60_000_000", "t = s.swapcase()", 1),
    (Row::Bounded, "s = 'AZ[' * 20_000_000", "t = s.lower()", 1),
    (Row::Bounded, "s = 'Σa' * 10_000_000", "t = s.lower()", 1),
    (Row::Bounded, "s = 'a' * 60_000_000", "t = s.title()", 1),
    (
        Row::Bounded,
        "s = '\\t' * 8_000_000",
        "t = s.expandtabs(8)",
        1,
    ),
    (
        Row::Bounded,
        "s = 'abc' * 20_000_000",
        "t = s.translate({97: 'x'})",
        1,
    ),
    // namedtuple's NFKC check: a name per step (`str.translate` with
    // `_mm.nfkc_table`), the duplicates in a set. One native over every
    // name ran 3.9 s for 50,000 and 3.7 s for 120 names of 1 MB
    // [M, reviewer].
    (
        Row::Bounded,
        "from collections import namedtuple\nnames = ['ｆ%d' % i for i in range(50_000)] + ['f0']",
        "try:\n    namedtuple('T', names)\nexcept SyntaxError:\n    pass",
        1,
    ),
    (
        Row::Bounded,
        "from collections import namedtuple\np = 'ｘ' * 333_333\nnames = [p + '%03d' % i for i in range(125)] + ['x' * 333_333 + '000']",
        "try:\n    namedtuple('T', names)\nexcept SyntaxError:\n    pass",
        1,
    ),
    (Row::Bounded, "s = 'a' * 60_000_000", "n = s.count('a')", 1),
    (
        Row::Bounded,
        "s = 'é' * 20_000_000",
        "b = s.isprintable()",
        1,
    ),
    (Row::Bounded, "s = 'a' * 60_000_000", "t = s[::-1]", 1),
    (Row::Bounded, "s = 'é' * 20_000_000", "t = s[::2]", 1),
    (
        Row::Bounded,
        "s = 'a' * 60_000_000",
        "t = s.center(67_000_000)",
        1,
    ),
    (
        Row::Bounded,
        "s = 'a' * 60_000_000",
        "t = format(s, '>67000000')",
        1,
    ),
    (Row::Bounded, "s = 'a' * 60_000_000", "t = '<%s>' % s", 1),
    (Row::Bounded, "s = 'é' * 16_000_000", "t = ascii(s)", 1),
    (
        Row::Bounded,
        "s = 'a' + ' ' * 66_999_999",
        "t = s.strip()",
        1,
    ),
    (Row::Bounded, "s = 'a' * 67_000_000", "t = s.format()", 1),
    // A 64 MiB copy, a slice per step (`STR_FILL`): one copy was 41-50 ms
    // here, past the limit scaled to this machine (36 ms) [M].
    (Row::Bounded, "s = 'a' * 67_000_000", "t = s + 'x'", 1),
    (Row::Bounded, "s = 'a' * 67_000_000", "t = s[1:]", 1),
    (
        Row::Bounded,
        "s = 'b' + 'a' * 66_999_999",
        "t = s.removeprefix('b')",
        1,
    ),
    // A long affix is compared in place, not validated and copied before
    // the comparison. Both the matching and mismatching ends are late.
    (
        Row::Bounded,
        "s = 'a' * 67_108_864\np = 'a' * 67_108_863 + 'b'",
        "assert s.removeprefix(p) is s",
        1,
    ),
    (
        Row::Bounded,
        "p = 'a' * 67_108_863\ns = 'b' + p",
        "assert s.removesuffix(p) == 'b'",
        1,
    ),
    (
        Row::Bounded,
        "s = ' ' + 'a' * 66_999_999",
        "t = s.strip()",
        1,
    ),
    // A long piece of a split or partition, copied a window per step into
    // its block (one copy: 48 ms here [M]).
    (
        Row::Bounded,
        "s = 'a' * 33_000_000 + 'b' + 'a' * 33_000_000",
        "t = s.partition('b')",
        1,
    ),
    (
        Row::Bounded,
        "s = 'a' * 33_000_000 + 'b' + 'a' * 33_000_000",
        "t = s.rpartition('b')",
        1,
    ),
    (
        Row::Bounded,
        "s = 'a' * 67_000_000 + ',b'",
        "t = s.split(',')",
        1,
    ),
    // The integrator's sweep (V8 and 1,000-unit fuel slices).
    (
        Row::Bounded,
        "s = 'ab' * 30_000_000",
        "t = s.translate({97: 'b'})",
        1,
    ),
    (
        Row::Bounded,
        "s = 'a' * 60_000_000",
        "t = s.replace('a', 'b')",
        1,
    ),
    (Row::Bounded, "s = 'a' * 60_000_000", "t = s.upper()", 1),
    (Row::Bounded, "s = 'a' * 60_000_000", "t = s % ()", 1),
    (
        Row::Bounded,
        "import html\ns = '<&>' * 4_000_000",
        "t = html.escape(s)",
        1,
    ),
    // A long container's repr goes on in the prelude twin, which joins its
    // pieces in runs (`_repr_run`), so collections mark a few long strs.
    (
        Row::Bounded,
        "d = dict.fromkeys(range(1_000_000))",
        "t = repr(d)",
        1,
    ),
    (
        Row::Bounded,
        "d = dict.fromkeys(range(1_000_000))",
        "t = ascii(d)",
        1,
    ),
    (
        Row::Bounded,
        "d = dict.fromkeys(range(1_000_000))",
        "t = f'{d}'",
        1,
    ),
    // P2's sweep: a long `bytes` repr goes as a str's (`repr` steps).
    (Row::Bounded, "B = b'a' * 60_000_000", "t = B % ()", 1),
    (Row::Bounded, "B = b'a' * 60_000_000", "t = str(B)", 1),
    // A lone f-string field is its formatted str, with no `BUILD_STRING`
    // copy (88 ms before [M]).
    (Row::Bounded, "B = b'a' * 60_000_000", "t = f'{B}'", 1),
    // A long `%` format is parsed a window per step (`nstep::op::STR_PCT`,
    // str and `bytes` alike).
    (
        Row::Bounded,
        "s = 'a' * 60_000_000 + '%s'",
        "t = s % ('x',)",
        1,
    ),
    (Row::Bounded, "s = '%%' * 30_000_000", "t = s % ()", 1),
    (
        Row::Bounded,
        "B = b'a' * 30_000_000 + b'%s'",
        "t = B % (b'z',)",
        1,
    ),
    (
        Row::Bounded,
        "s = '%d,' * 3_000_000\na = tuple(range(3_000_000))",
        "t = s % a",
        1,
    ),
    // `bytes.join` goes as a str join (`nstep::op::STR_JOIN`, a `bytes`
    // result); the longest step is the 65 MB block beside 1M live parts.
    (
        Row::Bounded,
        "xs = [('%064d' % i).encode() for i in range(1_000_000)]",
        "b = b'-'.join(xs)",
        1,
    ),
    (
        Row::Bounded,
        "xs = ['%064d' % i for i in range(1_000_000)]",
        "s = '-'.join(xs)",
        1,
    ),
    // The twin's `list(x)` of a 2M-item set goes a chunk per step
    // (`_collect_rest`); it was one collect step, 82-87 ms and 2.52M units
    // past a slice [M].
    (
        Row::Bounded,
        "st = set(range(2_000_000))",
        "t = repr(st)",
        1,
    ),
    // GC over millions of live objects: `gc.collect()` waits for a
    // collection that starts after it, a slice of the mark per pass of its
    // loop (70 ms in one step before).
    (
        Row::Bounded,
        "import gc\nxs = [[i] for i in range(4_000_000)]",
        "gc.collect()",
        1,
    ),
    // Collections the heap starts beside millions of live objects: a slice
    // of the mark per safepoint.
    (
        Row::Bounded,
        "xs = [[i] for i in range(2_000_000)]",
        "for i in range(6_000_000):\n    t = [i]",
        1,
    ),
    (
        Row::Bounded,
        "xs = [[i] for i in range(3_000_000)]",
        "for i in range(6_000_000):\n    t = [i]",
        1,
    ),
    // Collections beside 1,000 tasks, each blocked under 500 coroutine
    // frames (500,000 frames, about 244 MB): every collection step cleared
    // the dead registers of every frame of every task, and a step ran
    // 57.6-63.6 ms past a 1 ms deadline [M, plain release, one cpu]; now
    // only a step that takes roots clears, a live entry per frame.
    (
        Row::Bounded,
        "import asyncio",
        "async def leaf(ev):\n    await ev.wait()\n    return 1\n\
         async def rec(n, ev):\n    if n == 0:\n        return await leaf(ev)\n    \
         return await rec(n - 1, ev)\n\
         async def main():\n    ev = asyncio.Event()\n    \
         tasks = [asyncio.create_task(rec(500, ev)) for _ in range(1000)]\n    \
         await asyncio.sleep(0)\n    keep = []\n    i = 0\n    \
         while i < 1_000_000:\n        i += 1\n        keep.append([i, i + 1, i + 2])\n        \
         if len(keep) > 400_000:\n            keep = keep[200_000:]\n    \
         ev.set()\n    return sum(await asyncio.gather(*tasks))\n\
         r = asyncio.run(main())",
        1,
    ),
    // Scattered writes into a 199 MiB heap: the marks do not converge and
    // give up, and the next ones ran in full (65-67 ms on f305b88; 264-280
    // ms over 3M writes); they are now held, a slice per step (spec §9.2):
    // 3 ms [M].
    (
        Row::Bounded,
        "big = [[i, str(i)] for i in range(2_000_000)]\nn = len(big)",
        "for i in range(500_000):\n    big[(i * 7919) % n][1] = str(i)",
        1,
    ),
    // Containers (P4), at the largest sizes the 64 MiB object cap allows
    // (about 2M dict entries, 2.5M set items, 8M list items). Set and dict
    // operations a slice of items per step (`nstep::op::BULK`), their
    // tables rebuilt a slice per step as they grow; set `==` and `<=` a
    // native slice (`_more`) per loop step.
    (
        Row::Bounded,
        "xs = [(i, i) for i in range(2_000_000)]",
        "d = dict(xs)",
        1,
    ),
    (
        Row::Bounded,
        "xs = list(range(2_000_000))",
        "d = dict.fromkeys(xs)",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: i for i in range(2_000_000)}",
        "e = d.copy()",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: i for i in range(2_000_000)}\ne = {-1: 0}",
        "e.update(d)",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: i for i in range(2_000_000)}",
        "e = d | {0: 1}",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: i for i in range(2_000_000)}",
        "e = {**d}",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: i for i in range(2_000_000)}",
        "k = d.keys() - {1}",
        1,
    ),
    // Builds 2M new tuples, each charged as six items: 46-62 ms steps at
    // one unit each, about 7 ms now [M].
    (
        Row::Bounded,
        "d = {i: i for i in range(2_000_000)}",
        "k = d.items() | {(1, 1)}",
        1,
    ),
    // Peaks near the limit (it completes from a 244 MiB one) and ends at
    // about 195 MiB live [M], where collections used to run in full
    // (130-213 ms, 41-52 ms on f305b88); with too little room the mark is
    // now held, a slice per step (spec §9.2), and the test asserts that a
    // park held it ([`HELD_MARK_ROW`]). Under incremental marks it
    // raised `MemoryError` while a mark kept the mark stack at twice the
    // objects it had to hold. Its worst step was then the mark step after
    // the result's 32 MiB of hashes, paced to mark all of the heap
    // (30-35 ms); now the first inserts, which fault those hashes in
    // (24-26 ms) [M]. Its set of 2M pairs is no longer presized, as
    // CPython's `dictviews_to_set` does not presize for an items view: its
    // last growth holds the old 32 MiB table beside the new one, so `e` has
    // 500,000 pairs, not 1,000,000 (800,000 raise `MemoryError` now) [M].
    (
        Row::Bounded,
        "d = {i: i for i in range(2_000_000)}\ne = {(i, i) for i in range(500_000)}",
        HELD_MARK_ROW,
        1,
    ),
    // A view with a set subclass's instance or a generator as the other
    // operand (V5): the view was read in one native step, 79-291 ms at
    // 1M entries [M, review]; the twin's steps park now.
    (
        Row::Bounded,
        "d = dict.fromkeys(range(1_000_000))\nclass SS(set): pass\nss = SS(range(1_000_000))",
        "k = d.keys() - ss",
        1,
    ),
    (
        Row::Bounded,
        "d = dict.fromkeys(range(1_000_000))\nclass SS(set): pass\nss = SS(range(1_000_000))",
        "k = d.items() | ss",
        1,
    ),
    (
        Row::Bounded,
        "d = dict.fromkeys(range(1_000_000))",
        "k = d.keys() | (i for i in range(3))",
        1,
    ),
    // GC: deepcopy's memo dict is one large table. Each insert dirties its
    // segment, every rescan pass rescans the whole table, and the mark
    // gives up at a quarter of its first mark; it ran full collections
    // then (105-156 ms at 164-179 MiB live). A mark that gives up is now
    // held, a slice per step (spec §9.2): 25-26 ms [M].
    (
        Row::Bounded,
        "import copy\nold = [[i] for i in range(1_000_000)]\nxs = [[i, (i,)] for i in range(600_000)]",
        "ys = copy.deepcopy(xs)",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(1_250_000))\nb = set(range(1_250_000, 2_500_000))",
        "c = a | b",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(2_500_000))\nb = set(range(1_250_000, 3_750_000))",
        "c = a - b",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(1_250_000))\nb = set(range(1_250_000, 2_500_000))",
        "c = a ^ b",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(2_500_000))\nb = set(range(1_250_000, 3_750_000))",
        "a &= b",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(1_250_000))\nb = set(range(1_250_000, 2_500_000))",
        "a ^= b",
        1,
    ),
    // Presized to 2^23 slots, as CPython does; its two 64 MiB buffers come
    // in two steps; the collections during growth mark a slice per step,
    // each in a gap of its own (spec §9.2): 28 ms [M]. The keys table
    // comes unfilled and is marked empty a slice per step, without the
    // write barrier, so the forced mark finish at the end no longer
    // rescans it: worst gap 26-30 -> 12-13 ms alone [M].
    (
        Row::Bounded,
        "a = set(range(1_250_000))\nb = set(range(1_250_000, 2_500_000))",
        "a |= b",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(2_500_000))\nb = set(range(2_500_000))",
        "c = a == b",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(2_500_000))\nb = set(range(2_500_000))",
        "c = a <= b",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(2_500_000))\nxs = list(range(2_500_000))",
        "c = a.issubset(xs)",
        1,
    ),
    (
        Row::Bounded,
        "xs = list(range(2_500_000))",
        "a = set(xs)",
        1,
    ),
    (Row::Bounded, "", "a = set(range(2_500_000))", 1),
    (
        Row::Bounded,
        "a = set(range(2_500_000))\nxs = list(range(1_250_000, 3_750_000))",
        "c = a.intersection(xs)",
        1,
    ),
    // Variadic set methods drive one bounded operation per source. The
    // later arguments are large too, including beyond three sources.
    (
        Row::Bounded,
        SET_METHOD_SETUP,
        "s.update(a, iter(b), (), (), b)",
        1,
    ),
    (
        Row::Bounded,
        SET_METHOD_SETUP,
        "r = s.union(a, iter(b), (), (), b)",
        1,
    ),
    (
        Row::Bounded,
        SET_METHOD_SETUP,
        "s.intersection_update(a, b)",
        1,
    ),
    (
        Row::Bounded,
        SET_METHOD_SETUP,
        "r = s.intersection(a, b)",
        1,
    ),
    (
        Row::Bounded,
        SET_METHOD_SETUP,
        "s.difference_update(a, b)",
        1,
    ),
    (Row::Bounded, SET_METHOD_SETUP, "r = s.difference(a, b)", 1),
    // A Python iterator's deletes are already stepped; settling their
    // dummies at the end must not rebuild its whole table in one step.
    (
        Row::Bounded,
        "s = set(range(2_000_000))",
        "s.difference_update((i for i in range(1_100_000)), ())",
        1,
    ),
    (
        Row::Bounded,
        "a = set(range(2_000_000))",
        "c = a.union()",
        1,
    ),
    (Row::Bounded, "a = set(range(2_000_000))", "c = {*a}", 1),
    (Row::Bounded, "a = set(range(2_500_000))", "a.clear()", 1),
    // Lists and tuples built from slices of others a batch per step
    // (`nstep::op::SEQ_BUILD`); `count`, `index`, `remove` a slice per step
    // (`nstep::op::SEQ_SCAN`).
    (
        Row::Bounded,
        "xs = list(range(8_000_000))",
        "n = xs.count(-1)",
        1,
    ),
    (
        Row::Bounded,
        "t = tuple(range(8_000_000))",
        "try:\n    t.index(-1)\nexcept ValueError:\n    pass",
        1,
    ),
    (
        Row::Bounded,
        "xs = list(range(8_000_000))",
        "xs.remove(7_999_999)",
        1,
    ),
    (
        Row::Bounded,
        "xs = list(range(8_000_000))\nys = []",
        "ys.extend(xs)",
        1,
    ),
    // `xs += xs`, `ys = xs + xs` and `u = t * 2`: the 64 MiB result makes
    // a full collection of the 32 MiB source due, and the two shared a gap
    // from 68a955d until a park went ahead of a collection step (spec
    // §9.2). Worst gaps on d455c5e -> now [M]: 43-46 -> 26-30 ms,
    // 44-49 -> 28-36 ms and 44-46 -> 28-30 ms; the allocation is about
    // 25 ms and the collection (16-18 ms) a gap of its own. With
    // collections off the allocation alone was 20-25 ms (fresh pages).
    // A list or tuple built in steps now comes unfilled, so its pages
    // fault in as the steps write them: that gap is under 1 ms, and the
    // collection (15-19 ms) is the worst [M].
    (Row::Bounded, "xs = list(range(4_000_000))", "xs += xs", 1),
    (
        Row::Bounded,
        "xs = list(range(4_000_000))",
        "ys = xs + xs",
        1,
    ),
    (Row::Bounded, "t = tuple(range(4_000_000))", "u = t * 2", 1),
    // Step 0 is one 64 MiB alloc_object: fresh pages plus the UNDEFINED
    // fill (~40 ms on gVisor); single-run outliers to ~57 ms. mm-core
    // allocation floor, not the fill.
    (Row::Bounded, "", "ys = [0, 1] * 4_000_000", 1),
    // Step 0 is one 64 MiB alloc_object: fresh pages plus the UNDEFINED
    // fill (~40 ms on gVisor); single-run outliers to ~57 ms. mm-core
    // allocation floor, not the fill.
    (Row::Bounded, "", "ys = [0] * 8_000_000", 1),
    (
        Row::Bounded,
        "xs = list(range(8_000_000))\nys = list(range(8_000_000))",
        "xs[:] = ys",
        1,
    ),
    (
        Row::Bounded,
        "xs = list(range(8_000_000))\nys = list(range(4_000_000))",
        "xs[::2] = ys",
        1,
    ),
    (
        Row::Bounded,
        "xs = list(range(8_000_000))",
        "del xs[::2]",
        1,
    ),
    (
        Row::Bounded,
        "xs = list(range(8_000_000))",
        "ys = xs[::-1]",
        1,
    ),
    (
        Row::Bounded,
        "xs = list(range(8_000_000))",
        "ys = tuple(xs)",
        1,
    ),
    (
        Row::Bounded,
        "xs = list(range(8_000_000))",
        "xs.insert(0, 1)",
        1,
    ),
    (Row::Bounded, "xs = list(range(8_000_000))", "xs.pop(0)", 1),
    // The tail moves in place, one `memmove` of up to 64 MiB (about 7 ms
    // [M]); `insert` into a full buffer still grows it in steps.
    (
        Row::Bounded,
        "xs = list(range(8_000_000))\nxs.append(0)",
        "xs.insert(0, 1)",
        1,
    ),
    (Row::Bounded, "xs = list(range(8_000_000))", "del xs[0]", 1),
    // An items view read in place, not collected first (one 145 ms step).
    (
        Row::Bounded,
        "d = {i: i for i in range(2_000_000)}",
        "e = dict(d.items())",
        1,
    ),
    (
        Row::Bounded,
        "d = {i: i for i in range(2_000_000)}",
        "s = set(d.items())",
        1,
    ),
    // Number text (P3): `int()`, `float()` and `complex()` read a text a
    // piece per step (`nstep::op::NUM_TEXT`), long digits are converted a
    // piece per step. Texts at the 64 MiB object cap.
    (
        Row::Bounded,
        "s = '1' * 67_000_000 + 'x'",
        "try:\n    int(s)\nexcept ValueError:\n    pass",
        1,
    ),
    (
        Row::Bounded,
        "s = '\u{0661}' * 30_000_000 + 'x'",
        "try:\n    int(s)\nexcept ValueError:\n    pass",
        1,
    ),
    // A 2^26-bit result (`intops::MAX_BITS`), and a small one after 64 MiB
    // of leading zeros.
    (Row::Bounded, "s = 'f' * 16_777_216", "x = int(s, 16)", 1),
    (
        Row::Bounded,
        "s = '0' * 67_000_000 + '1'",
        "x = int(s, 16)",
        1,
    ),
    (
        Row::Bounded,
        "import sys\nsys.set_int_max_str_digits(0)\ns = '7' * 300_000",
        "x = int(s)",
        1,
    ),
    (Row::Bounded, "s = '1' * 67_000_000", "f = float(s)", 1),
    (
        Row::Bounded,
        "s = '1' * 67_000_000 + 'x'",
        "try:\n    complex(s)\nexcept ValueError:\n    pass",
        1,
    ),
    // The message holds the whole repr, built in `_num_error` by `repr`
    // and `+`, both in steps (P1). At 100 MB the text, its repr and the
    // message do not fit the limit (`MemoryError`).
    (
        Row::Bounded,
        "s = '1' * 40_000_000 + 'x'",
        "try:\n    float(s)\nexcept ValueError:\n    pass",
        1,
    ),
    // The math folds (P3): a slice of items per step
    // (`nstep::op::MATH_FOLD`).
    (
        Row::Bounded,
        "import math\nxs = [1.0] * 8_000_000",
        "y = math.fsum(xs)",
        1,
    ),
    (
        Row::Bounded,
        "import math\nxs = [1.5] * 8_000_000",
        "y = math.prod(xs)",
        1,
    ),
    (
        Row::Bounded,
        "import math\nxs = [1.0] * 7_000_000\nys = [2.0] * 7_000_000",
        "y = math.sumprod(xs, ys)",
        1,
    ),
    (
        Row::Bounded,
        "import math\nxs = [1.0] * 7_000_000\nys = [2.0] * 7_000_000",
        "y = math.dist(xs, ys)",
        1,
    ),
    // `math.hypot(*xs)`: `[*xs]` and its tuple are built a slice per step
    // (`SEQ_BUILD`), and the fold reads the argument tuple itself instead
    // of a copy laid out on the stack in one go (418 ms, then
    // `MemoryError`, on eb78933 [M]).
    (
        Row::Bounded,
        "import math\nxs = [1.5] * 8_000_000",
        "y = math.hypot(*xs)",
        1,
    ),
    // `float.fromhex` in one go, a word at a time, at the 64 MiB object
    // cap: a sticky digit after a long zero run, and an all-zero tail.
    (
        Row::Bounded,
        "s = '0x1.' + '0' * 67_000_000 + '1p0'",
        "f = float.fromhex(s)",
        1,
    ),
    (
        Row::Bounded,
        "s = '0x1' + '0' * 67_000_000 + 'p0'",
        "try:\n    f = float.fromhex(s)\nexcept OverflowError:\n    pass",
        1,
    ),
    // `int.to_bytes` at the 64 MiB object cap: a negative int's sign
    // padding is filled a step at a time (`bytes::job::PAD`). In one go,
    // 64 MiB of fresh pages took 41-48 ms on a quiet core; 3.6-6.6 ms a
    // step now [M].
    (
        Row::Bounded,
        "x = -5",
        "b = x.to_bytes(67_108_000, 'big', signed=True)",
        1,
    ),
    // `int.from_bytes` of a `bytearray` at the int cap (8 MiB) reads its
    // block in place, as of `bytes` (183-195 ms item by item before;
    // review T2).
    (
        Row::Bounded,
        "b = bytearray(b'\\x01') * (8 << 20)",
        "n = int.from_bytes(b, 'big')",
        1,
    ),
    // Power-of-two digits at the int cap (2^26 bits), in one go: the int
    // read in place and its digits written into an arena block, which
    // becomes the str (`Heap::str_from_block`).
    (Row::Bounded, "x = (1 << 67_000_000) - 1", "s = bin(x)", 1),
    (
        Row::Bounded,
        "x = -((1 << 67_108_000) - 1)",
        "s = oct(x)",
        1,
    ),
    (
        Row::Bounded,
        "x = -((1 << 67_108_000) - 1)",
        "s = hex(x)",
        1,
    ),
    // Superlinear int operations (P3): one whose
    // estimated time is past about 50 ms fails with `MemoryError` before
    // it starts (`intops::WORK_MAX`). Fuzz finding C5: these ran for
    // seconds with no safepoint.
    (
        Row::Bounded,
        "",
        "try:\n    x = 3 ** (2 ** 22)\nexcept MemoryError:\n    pass",
        1,
    ),
    (
        Row::Bounded,
        "x = (1 << 6_650_000) // 3",
        "try:\n    y = x * x\nexcept MemoryError:\n    pass",
        1,
    ),
    (
        Row::Bounded,
        "x = (1 << 67_000_000) // 3\ny = (1 << 100_000) // 7",
        "try:\n    q = divmod(x, y)\nexcept MemoryError:\n    pass",
        1,
    ),
    (
        Row::Bounded,
        "import math",
        "try:\n    y = math.factorial(400_000)\nexcept MemoryError:\n    pass",
        1,
    ),
    // C5 on c0e4045 (host1's release fuzz write-up): `3 ** (2 ** 23)` ran
    // 1.69 s past a 50 ms deadline, and a squaring loop returned 0.69 s
    // after a 1 s one. Both now end in `MemoryError` at the cap.
    (
        Row::Bounded,
        "",
        "try:\n    x = 3 ** (2 ** 23)\nexcept MemoryError:\n    pass",
        1,
    ),
    (
        Row::Bounded,
        "x = 3",
        "try:\n    while True:\n        x = x * x\nexcept MemoryError:\n    pass",
        1,
    ),
    // The overdraft sweep at `7 ** 1_000_000`'s size (2,807,355 bits):
    // `gcd` with a small argument is charged about linearly (was 962M
    // units past a slice for 0.9 ms of work); `isqrt` fails before it
    // starts (was one 44 s step after a fuel park).
    (
        Row::Bounded,
        "import math\nR = ((1 << 2_807_355) - 1) // 3",
        "y = math.gcd(R, 6)",
        1,
    ),
    (
        Row::Bounded,
        "import math\nR = ((1 << 2_807_355) - 1) // 3",
        "try:\n    y = math.isqrt(R)\nexcept MemoryError:\n    pass",
        1,
    ),
    // HANDOFF #9: a modulus of up to 128 bits is a short division by
    // reciprocals (`bigint::divmod_word`, `divmod_dword`), so the int cap
    // divides by any such one, as CPython does; algorithm D's estimate
    // refused every one from 2^32 there. `gcd` estimates its first
    // division and then Euclid on the reduced pair (it refused `m = 3` at
    // 3 * 2^24 bits).
    (
        Row::Bounded,
        "x = (1 << 67_108_800) - 12_345\nm = (1 << 64) - 1",
        "q, r = divmod(x, m)",
        1,
    ),
    (
        Row::Bounded,
        "x = (1 << 67_108_800) - 12_345\nm = (1 << 127) - 1",
        "r = -x % m",
        1,
    ),
    (
        Row::Bounded,
        "x = (1 << 67_108_800) - 12_345",
        "y = pow(x, 2, 1 << 64)",
        1,
    ),
    (
        Row::Bounded,
        "import math\nx = (1 << 67_108_800) - 12_345",
        "y = math.gcd(x, (1 << 64) + 1)",
        1,
    ),
    // Many arguments at the cap (review H2): each was let through on its
    // own estimate and all ran in one step, 659 ms for 40. Past the
    // summed estimate the prelude goes pairwise, a step per pair. `lcm`
    // divides the shorter operand by the gcd, not the product: 54 ms for
    // `lcm(x, 3)` at the cap, about 33 now [M].
    (
        Row::Bounded,
        "import math\nx = (1 << 67_108_800) - 12_345",
        "y = math.gcd(3, *[x] * 40)",
        1,
    ),
    (
        Row::Bounded,
        "import math\nx = (1 << 67_108_800) - 12_345",
        "y = math.lcm(x, *[3, 5, 7] * 10)",
        1,
    ),
    // Bytes (P2): `nstep::op::BYTES`, a `bytes::Job` of each kind, its
    // result built in place in an arena block of its final size; each at
    // the 64 MiB object cap (operand or result).
    (
        Row::Bounded,
        "b = b'a' * 33_554_432",
        "t = b.replace(b'a', b'xy')",
        1,
    ),
    (
        Row::Bounded,
        "b = b'abcdefg ' * 3_000_000",
        "xs = b.split(b' ')",
        1,
    ),
    // `bytes.join` (P1's `STR_JOIN`) over 8M small parts.
    (
        Row::Bounded,
        "xs = [b'ab'] * 8_000_000",
        "t = b' '.join(xs)",
        1,
    ),
    (Row::Bounded, "b = b'ab'", "t = b * 33_554_432", 1),
    (
        Row::Bounded,
        "b = b'a' * 67_108_864",
        "n = b.count(b'a')",
        1,
    ),
    (
        Row::Bounded,
        "b = b'ab' * 33_554_432",
        "t = b.strip(b'ba')",
        1,
    ),
    (
        Row::Bounded,
        "b = b'ab' * 33_554_432\ntb = bytes.maketrans(b'ab', b'ba')",
        "t = b.translate(tb)",
        1,
    ),
    (Row::Bounded, "b = b'ab' * 33_554_432", "t = b.upper()", 1),
    (
        Row::Bounded,
        "b = b'ab' * 33_554_432",
        "s = b.decode('latin-1')",
        1,
    ),
    (
        Row::Bounded,
        "s = 'ab' * 16_777_215",
        "b = s.encode('utf-16')",
        1,
    ),
    (Row::Bounded, "b = b'ab' * 16_777_216", "t = b.hex()", 1),
    (
        Row::Bounded,
        "import base64\nb = base64.b64encode(b'ab' * 25_165_824)",
        "t = base64.b64decode(b)",
        1,
    ),
    (
        Row::Bounded,
        "import binascii\nb = b'ab' * 33_554_432",
        "t = binascii.crc32(b)",
        1,
    ),
    (
        Row::Bounded,
        "import hashlib\nb = b'ab' * 33_554_432\nh = hashlib.sha256()",
        "h.update(b)",
        1,
    ),
    (
        Row::Bounded,
        "import hashlib",
        "d = hashlib.shake_128(b'x').digest(67_108_864)",
        1,
    ),
    (Row::Bounded, "b = bytearray(67_108_864)", "del b[::2]", 1),
    // A late byte match, a close overlapping move over demand-zero
    // pages, and a reversal that moves a filled half into fresh pages.
    (
        Row::Bounded,
        "b = bytearray(67_108_864)\nb[-2:] = b'xx'",
        "b.remove(120)\nassert len(b) == 67_108_863 and b[-2:] == b'\\x00x'",
        1,
    ),
    (
        Row::Bounded,
        "b = bytearray(67_108_864)\nb[0] = 1\nb[4096] = 7",
        "assert b.pop(0) == 1 and b[4095] == 7 and len(b) == 67_108_863",
        1,
    ),
    (
        Row::Bounded,
        "b = bytearray(67_108_863)\nb[:33_554_432] = b'ab' * 16_777_216",
        "b.reverse()\nassert b[:2] == b'\\x00\\x00' and b[-2:] == b'ba' and b[33_554_431] == 98",
        1,
    ),
    // A str strip's membership bitmap, built a slice at a time from a
    // long `chars`, then a short receiver trimmed on a miss or late hits
    // (227 ms a step and 8.4 s a step by a linear search [M]).
    (
        Row::Bounded,
        "d = 'ab' * 33_554_431 + 'c'\ns = 'c' * 200",
        "assert s.strip(d) == ''",
        1,
    ),
    (
        Row::Bounded,
        "d = 'ab' * 33_554_432\ns = 'z' * 200",
        "assert s.strip(d) == s",
        1,
    ),
    (
        Row::Bounded,
        "d = '𐐷éa' * 9_586_980 + '🙂'",
        "assert '🙂éx𐐷'.strip(d) == 'x'",
        1,
    ),
    // A receiver just under the long-text size trimmed through a bitmap:
    // its windows are charged as they go, so a step ends at its budget
    // (24.4 ms in one stretch when the charge was deferred [M]).
    (
        Row::Bounded,
        "s = 'ж' * 2_097_000 + 'x'",
        "assert s.lstrip('жa') == 'x'",
        1,
    ),
    // A long receiver trimmed through a bitmap of 256 chars.
    (
        Row::Bounded,
        "d = ''.join(map(chr, range(0x400, 0x500)))\ns = 'ж' * 33_554_431 + 'x'",
        "assert s.lstrip(d) == 'x'",
        1,
    ),
    // Preparing the byte set can read a full object even when the data
    // to translate or strip is tiny.
    (
        Row::Bounded,
        "d = b'ab' * 33_554_432",
        "assert b'zaab'.translate(None, d) == b'z'",
        1,
    ),
    (
        Row::Bounded,
        "d = b'ab' * 33_554_432",
        "assert b'azb'.strip(d) == b'z'",
        1,
    ),
    // `strptime`'s ASCII native fast path, not the regex fallback for
    // unsupported formats. Both input and format can have a long run.
    #[cfg(all(feature = "mod-time", feature = "mod-datetime"))]
    (
        Row::Bounded,
        "from time import strptime\nstrptime('2024', '%Y')\ns = ' \\t\\v\\x1c' * 16_777_215 + '2024'",
        "assert strptime(s, ' %Y').tm_year == 2024",
        1,
    ),
    #[cfg(all(feature = "mod-time", feature = "mod-datetime"))]
    (
        Row::Bounded,
        "from time import strptime\nstrptime('2024', '%Y')\nf = ' \\t\\v\\x1c' * 16_777_215 + '%Y'",
        "assert strptime(' 2024', f).tm_year == 2024",
        1,
    ),
    // A tuple can compare the same large candidate many times. Its
    // progress is bounded both inside one comparison and between items.
    (
        Row::Bounded,
        "s = 'a' * 33_554_432\np = 'a' * 33_554_431 + 'b'\nt = (p,) * 128",
        "assert not s.startswith(t)",
        1,
    ),
    (
        Row::Bounded,
        "s = 'a' * 33_554_432\np = 'a' * 33_554_431 + 'b'\nt = (p,) * 128",
        "assert not s.endswith(t)",
        1,
    ),
    (
        Row::Bounded,
        "s = b'a' * 33_554_432\np = b'a' * 33_554_431 + b'b'\nt = (p,) * 128",
        "assert not s.startswith(t)",
        1,
    ),
    (
        Row::Bounded,
        "s = bytearray(b'a' * 33_554_432)\np = b'a' * 33_554_431 + b'b'\nt = (p,) * 128",
        "assert not s.endswith(t)",
        1,
    ),
    // A compact repeat still has one conversion per scalar. Pass the
    // prepared tuple directly to the pack native to isolate struct work
    // from expansion of a multi-million-argument Python call. A long run
    // of redundant digits must also step even though it packs one item.
    #[cfg(feature = "mod-struct")]
    (
        Row::Bounded,
        "from struct import _pack\na = (3,) * 4_000_000",
        "b = _pack('<4000000I', a)\nassert len(b) == 16_000_000 and b[-4:] == b'\\x03\\x00\\x00\\x00'",
        1,
    ),
    #[cfg(feature = "mod-struct")]
    (
        Row::Bounded,
        "import struct\nb = b'\\x03\\x00\\x00\\x00' * 4_000_000",
        "a = struct.unpack('<4000000I', b)\nassert len(a) == 4_000_000 and a[-1] == 3",
        1,
    ),
    #[cfg(feature = "mod-struct")]
    (
        Row::Bounded,
        "import struct\nf = b'0' * 33_554_432 + b'1B'",
        "assert struct.pack(f, 3) == b'\\x03'",
        1,
    ),
    // A single large struct field must not debit its entire byte cost
    // before the next safepoint.
    #[cfg(feature = "mod-struct")]
    (
        Row::Bounded,
        "import struct\nb = b'ab' * 33_554_432",
        "out = struct.pack('67108864s', b)\nassert out[:2] == b'ab' and out[-2:] == b'ab' and len(out) == len(b)",
        1,
    ),
    #[cfg(feature = "mod-struct")]
    (
        Row::Bounded,
        "import struct\nb = b'ab' * 33_554_432",
        "(out,) = struct.unpack('67108864s', b)\nassert out[:2] == b'ab' and out[-2:] == b'ab' and len(out) == len(b)",
        1,
    ),
    // A wide field in a mixed record steps both with padding and with
    // real scalar values immediately before and after it.
    #[cfg(feature = "mod-struct")]
    (
        Row::Bounded,
        "import struct\nb = b'ab' * 33_554_431 + b'a'",
        "out = struct.pack('67108863sx', b)\nassert len(out) == len(b) + 1 and out[-2:] == b'a\\x00'",
        1,
    ),
    #[cfg(feature = "mod-struct")]
    (
        Row::Bounded,
        "import struct\nb = b'ab' * 33_554_432",
        "(out,) = struct.unpack('67108863sx', b)\nassert len(out) == 67_108_863 and out[-2:] == b'ba'",
        1,
    ),
    #[cfg(feature = "mod-struct")]
    (
        Row::Bounded,
        "import struct\nb = b'ab' * 33_554_428",
        "out = struct.pack('<I67108856sI', 3, b, 5)\nassert len(out) == 67_108_864 and out[:6] == b'\\x03\\x00\\x00\\x00ab' and out[-6:] == b'ab\\x05\\x00\\x00\\x00'",
        1,
    ),
    #[cfg(feature = "mod-struct")]
    (
        Row::Bounded,
        "import struct\nb = b'\\x03\\x00\\x00\\x00' * 16_777_216",
        "a, out, z = struct.unpack('<I67108856sI', b)\nassert a == z == 3 and len(out) == 67_108_856 and out[-4:] == b'\\x03\\x00\\x00\\x00'",
        1,
    ),
    // A two-way search whose every alignment fails late (the needle's
    // critical factorization).
    (
        Row::Bounded,
        "b = b'a' * 67_108_864",
        "i = b.find(b'aaab')",
        1,
    ),
    // A list of ints read in place (`job::INTS`), into a new result and
    // into a bytearray.
    (Row::Bounded, "xs = [1] * 8_388_608", "t = bytes(xs)", 1),
    (
        Row::Bounded,
        "xs = [1] * 8_388_608; b = bytearray(b'a')",
        "b.extend(xs)",
        1,
    ),
    // A full bytearray growing into a new block at the 64 MiB cap
    // (`job::GROW`), at its end, at its start, by another 32 MiB.
    (
        Row::Bounded,
        "b = bytearray(b'a' * 67_108_863)",
        "b.append(1)",
        1,
    ),
    (
        Row::Bounded,
        "b = bytearray(b'a' * 67_108_863)",
        "b.insert(0, 1)",
        1,
    ),
    (
        Row::Bounded,
        "b = bytearray(b'a' * 33_554_432); c = b'c' * 33_554_432",
        "b += c",
        1,
    ),
    // A bytearray with spare capacity opening a gap at its start: the
    // tail moves up over itself in place, one step (`Heap::copy_bytes`),
    // over pages never written. musl's `memmove` copies it backwards
    // (71 ms [M]); there it moves in pieces, 32 bytes at a time for a
    // narrow gap and the gap's length at a time for a wide one. What is
    // left is writing 64 MiB of new pages: 96 to 97% of the limit with
    // glibc and with musl's `malloc`, and over it on glibc in 3 of 15
    // runs [M], so the rows are `Open` (CHANGELOG, Known issues) until
    // the move goes a step at a time.
    (
        Row::Open,
        "b = bytearray(67_108_864)\nb.pop()",
        "b.insert(0, 1)",
        1,
    ),
    (
        Row::Open,
        "b = bytearray(67_108_864)\nb.pop()",
        "b[0:0] = b'x'",
        1,
    ),
    (
        Row::Open,
        "b = bytearray(67_108_864)\ndel b[-8192:]",
        "b[0:0] = b'x' * 8192",
        1,
    ),
    // The overdraft sweep's bytes sites (60 MB, or the most the 64 MiB cap
    // allows the result).
    (
        Row::Bounded,
        "b = b'abcdefg\\t' * 7_000_000",
        "t = b.expandtabs()",
        1,
    ),
    (
        Row::Bounded,
        "import urllib.parse; b = b'a' * 60_000_000",
        "t = urllib.parse.quote(b)",
        1,
    ),
    (
        Row::Bounded,
        "import binascii; b = b'a' * 33_554_432",
        "t = binascii.hexlify(b)",
        1,
    ),
    (
        Row::Bounded,
        "import base64; b = b'a' * 50_331_648",
        "t = base64.b64encode(b)",
        1,
    ),
    (Row::Bounded, "b = b'a' * 22_369_621", "t = b.hex(b':')", 1),
    // A set of ints read in place, failing late.
    (
        Row::Bounded,
        "xs = frozenset(range(256)) | frozenset(range(10**6, 3 * 10**6))",
        "try:\n    bytes(xs)\nexcept ValueError:\n    pass",
        1,
    ),
    // json (P5). `loads`: long tokens a slice a step, containers left open
    // across steps; texts up to the 64 MiB object cap.
    (
        Row::Bounded,
        "import json\ns = '\"' + 'a\\\\n' * 15_000_000 + '\"'",
        "t = json.loads(s)",
        1,
    ),
    (
        Row::Bounded,
        "import json\ns = '\"' + '\\xe9' * 30_000_000 + '\"'",
        "t = json.loads(s)",
        1,
    ),
    (
        Row::Bounded,
        "import json\ns = '1' * 67_000_000",
        "try:\n    json.loads(s)\nexcept ValueError:\n    pass",
        1,
    ),
    (
        Row::Bounded,
        "import json\ns = '1.' + '1' * 67_000_000",
        "t = json.loads(s)",
        1,
    ),
    (
        Row::Bounded,
        "import json\ns = '[' * 99_999 + '1' + ', 2' * 3_000_000 + ']' * 99_999",
        "t = json.loads(s)",
        1,
    ),
    // A big object ends a step before its entries could fill its table,
    // and the next steps grow it a slice at a time (`Vm::dict_grow_slice`;
    // the 2^20 to 2^21 rebuild was one 66-84 ms step [M]). 2M keys, worst
    // step 82-84 -> 25 ms, 1M keys 26.7 ms, 600k keys 22.7 ms [M].
    (
        Row::Bounded,
        "import json\ns = json.dumps({str(i): i for i in range(600_000)})",
        "d = json.loads(s)",
        1,
    ),
    (
        Row::Bounded,
        "import json\ns = json.dumps({str(i): i for i in range(1_000_000)})",
        "d = json.loads(s)",
        1,
    ),
    (
        Row::Bounded,
        "import json\ns = json.dumps({str(i): i for i in range(2_000_000)})",
        "d = json.loads(s)",
        1,
    ),
    // A child spanning a step closes into a nearly full object: the step
    // ends before the close and the object's table grows first.
    (
        Row::Bounded,
        "import json\ns = json.dumps({str(i): list(range(10)) for i in range(300_000)})",
        "d = json.loads(s)",
        1,
    ),
    // `dumps` (`nstep::op::JSON_DUMP`): containers and text kept across
    // steps, a long str a slice a step, a large dict's keys sorted by the
    // prelude, the text written into one byte block. A text of 4 MiB or
    // more grows by moving to a new block 4 MiB a step, since growing in
    // place may copy the whole block in one step (glibc after a large
    // free, dlmalloc on wasm): the 7M-char row's last step was 36.6 ms in
    // the full run on 8262ec5 and 32-41 ms on wasm, 7.9 and 18.6-19.1 ms
    // moved [M]. When the limit cannot admit both blocks the move falls
    // back to growing in place, so a text near the limit may still copy
    // in one step.
    (
        Row::Bounded,
        "import json\nxs = list(range(5_000_000))",
        "s = json.dumps(xs)",
        1,
    ),
    (
        Row::Bounded,
        "import json\nxs = list(range(3_000_000))",
        "s = json.dumps(xs, indent=2)",
        1,
    ),
    // On musl its last steps, writing the sorted keys a few short appends
    // an item, took 47 to 56 ms against 27 to 30 on glibc: each append
    // was a libc `memcpy` call there (`StrBuf::push_str`). With fixed-size
    // copies, 25 to 46 ms on musl's `malloc` and 29 to 39 with mimalloc,
    // against limits of 53 to 58 [M, 12 alternated runs each].
    (
        Row::Bounded,
        "import json\nd = {str(i): i for i in range(1_000_000)}",
        "s = json.dumps(d, sort_keys=True, indent=1)",
        1,
    ),
    (
        Row::Bounded,
        "import json\ns = 'a' * 60_000_000",
        "t = json.dumps(s)",
        1,
    ),
    (
        Row::Bounded,
        "import json\nxs = {'\\xe9' * 7_000_000: ['\\n' * 7_000_000]}",
        "t = json.dumps(xs)",
        1,
    ),
    (
        Row::Bounded,
        "import json\nxs = [10 ** 3000] * 4_000",
        "t = json.dumps(xs)",
        1,
    ),
    // The integrator's overdraft sweep (152994a, unchunked: 5.6M to 8.4M
    // units past a slice, 155 to 317 ms): a tuple, and round trips.
    (
        Row::Bounded,
        "import json\nxs = tuple(range(4_000_000))",
        "t = json.dumps(xs)",
        1,
    ),
    (
        Row::Bounded,
        "import json\ns = json.dumps(list(range(4_000_000)))",
        "t = json.loads(s)",
        1,
    ),
    (
        Row::Bounded,
        "import json\ns = json.dumps('a' * 60_000_000)",
        "t = json.loads(s)",
        1,
    ),
    // A million small dicts: no collection or mark runs in the cell, and
    // the worst gap is a slice of `dumps` itself, 6-8 ms run alone (6-11 ms
    // with collections off) and 16 ms in the oracle [M]. It was 61-64 ms
    // on 8cd850c, before a park went ahead of a collection step and a
    // forced finish stopped giving up (spec §9.2). Then 40-58 ms: the
    // collection the text's moves request (`Vm::collect_for_result`) ran
    // in full, in one step; it marks incrementally now, 7-12 ms [M].
    (
        Row::Bounded,
        "import json\nxs = [{'a': i, 'b': [i, 'x'], 'c': None} for i in range(1_000_000)]",
        "s = json.dumps(xs)",
        1,
    ),
    // `tokenize` lexes in one step, its source capped at 64 KiB (P5,
    // decision (c)): a token a byte at the cap.
    (
        Row::Bounded,
        "import tokenize, io\nsrc = ('a;' * 31 + 'a\\n') * 1_024",
        "t = list(tokenize.generate_tokens(io.StringIO(src).readline))",
        1,
    ),
    (
        Row::Bounded,
        "import tokenize, io\nsrc = 'a\\n' * 32_768",
        "t = list(tokenize.generate_tokens(io.StringIO(src).readline))",
        1,
    ),
    // A join with bytearray parts or separator, read in place by
    // `STR_JOIN` (was one step: 118 ms and 3.1M units past a slice [M]).
    (
        Row::Bounded,
        "xs = [bytearray(b'%063d' % i) for i in range(1_000_000)]",
        "t = b'-'.join(xs)",
        1,
    ),
    (
        Row::Bounded,
        "xs = [b'%063d' % i for i in range(1_000_000)]",
        "t = bytearray(b'-').join(xs)",
        1,
    ),
    (
        Row::Bounded,
        "xs = [bytearray(b'a' * 16_777_215) for _ in range(4)]",
        "t = b'-'.join(xs)",
        1,
    ),
    // A dict or set growing by one insert at a time (P4): from 262,144
    // live entries (`set::GROW_STEPS`) the insert that needs room hands the
    // growth to BULK, the new tables a step each and the old entries a
    // slice per step. In one step, the dict's doubling at 1,048,576 took
    // 24-42 ms and a set's growth to 8M slots 90-101 ms [M]; now 10-16 ms
    // and 25-32 ms (the 64 MiB keys table's step) on an orb at 0.9 of the
    // reference [M]; since the keys table comes unfilled and is marked
    // empty a slice per step, 12-13 ms, a forced mark finish [M]. A key hashed in Python takes
    // the prelude twin, whose growth is one step (not measured here), as
    // does a growth with less than a 32nd of the limit left beside its new
    // tables (`Vm::grow_in_steps`). `SET_ADD` (a set comprehension or
    // display) steps from its out-of-line `set_add_inner`, with the arm
    // and `set_add` as before: 95 -> 32 ms [M]. Any shape that changed
    // what `Vm::dispatch` inlines moved wasmtime's loop bench, which runs
    // no `SET_ADD`, 13-16% (wasm-wall, 4 runs [M]).
    (
        Row::Bounded,
        "",
        "d = {}\nfor i in range(2_000_000):\n    d[i] = i",
        1,
    ),
    (Row::Bounded, "", "d = {i: i for i in range(2_000_000)}", 1),
    (
        Row::Bounded,
        "",
        "s = set()\nfor i in range(2_600_000):\n    s.add(i)",
        1,
    ),
    (Row::Bounded, "", "s = {i for i in range(2_600_000)}", 1),
    // A builtin iterator (a container's, `zip`, `enumerate`, `reversed`)
    // as the source of a set or dict is read in place an item at a time,
    // as CPython reads it, its tuples at `TUPLE_ITEM` units each; it was
    // collected into a list first. 16-17 ms on an orb at 0.77 of the
    // reference [M].
    (
        Row::Bounded,
        "xs = list(range(2_000_000))",
        "d = dict(enumerate(xs))",
        1,
    ),
    (
        Row::Bounded,
        "ks = list(range(2_000_000))",
        "d = dict.fromkeys(iter(ks))",
        1,
    ),
    // Straight-line code of small native charges (each at most 64 units):
    // `CHECK` runs the safepoint only once `Vm::burn` has shortened the
    // tick period, and a lone charge of 64 units or less did not, so a
    // long loop body reached its deadline, request and collection checks
    // once per 256 back edges. 900 slices of 59 units each: 117-129 ms
    // past the deadline on 99364cb; now `burn` counts the units since the
    // last check and checks past 64 [M].
    (
        Row::Bounded,
        "s = 'x' * 3840",
        include_str!("steps/slices.py"),
        1,
    ),
    // Two run_source fuzz inputs (host1): one 12 KB statement of big-int,
    // float and complex literal arithmetic as a loop body, 39-64 ms past
    // the deadline on 99364cb for the same reason [M].
    (
        Row::Bounded,
        "pass",
        include_str!("steps/run_source_126.py"),
        1,
    ),
    (
        Row::Bounded,
        "pass",
        include_str!("steps/run_source_e0c.py"),
        1,
    ),
    // The eb78933 review (S1): `str.format` with a short template formatted
    // long arguments natively, each copied in one step (142 ms on eb78933
    // [M]). A long argument, a wide spec or a long output now goes to the
    // prelude twin, whose pieces and join step; an empty spec gives a long
    // str itself, as CPython does: 4.7 ms [M].
    (
        Row::Bounded,
        "S = 'a' * 30_000_000",
        "t = '{}{}'.format(S, S)",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 30_000_000",
        "t = '{!r}'.format(S)",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000",
        "t = '{}x'.format(S)",
        1,
    ),
    (Row::Bounded, "", "t = '{:>67000000}'.format('a')", 1),
    // A number padded to a width of tens of millions is padded in steps
    // too (`Vm::num_pad`, below): 85 ms, the f-string 160 ms, in one step
    // before [M].
    (Row::Bounded, "", "t = format(5, '>67000000')", 1),
    (Row::Bounded, "", "t = f'{5:>67000000}'", 1),
    // (S2) `find`, `rfind`, `index` and `in` over a long window search a
    // window a step (`STR_SCAN` kinds `FIND`, `RFIND`), the window less the
    // pattern's length: 90-111 ms with a 1,000-char needle and 333-520 ms
    // with a short one whose first byte is everywhere on eb78933, now
    // 8-14 ms [M]. A short pattern's byte search turns to a word compare
    // per position when its first byte keeps matching.
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nN = 'a' * 999 + 'b'",
        "i = S.find(N)",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nN = 'a' * 999 + 'b'",
        "b = N in S",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nN = 'b' + 'a' * 999",
        "i = S.rfind(N)",
        1,
    ),
    (Row::Bounded, "S = 'a' * 67_000_000", "i = S.find('ab')", 1),
    (Row::Bounded, "S = 'a' * 67_000_000", "i = S.rfind('ba')", 1),
    (Row::Bounded, "S = 'a' * 67_000_000", "n = S.count('ab')", 1),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nN = 'a' * 8_000_000 + 'b'",
        "i = S.find(N)",
        1,
    ),
    // A needle of 512 KiB or more (P5): the bytes search, its
    // factorization kept between steps; each step read a 16M-char needle
    // whole (the two-way searcher's setup), 73 ms [M]. 5.3 ms now [M].
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nN = 'a' * 16_000_000 + 'b'",
        "i = S.find(N)",
        1,
    ),
    // (S3) A str's first hash reads the whole text: a unit per 64 bytes,
    // one 18 ms step at the 64 MiB cap [M], memcpy-class.
    (Row::Bounded, "S = 'a' * 67_000_000", "h = hash(S)", 1),
    // The review's T1: a short `%` format, `str.format` or container repr
    // over many big ints was one step of 1.4-3.3 s (113.8M units past a
    // slice). Each now stops by work, each conversion charged by its size
    // (`big_decimal`): a `%` format at a conversion past `STEP_WORK`
    // whatever its length, `str.format` handing off to its twin, the repr
    // walk's budget in units, not visits: 4-8 ms [M].
    (
        Row::Bounded,
        "L = [10**4000] * 10000",
        "t = ('%d' * 10000) % tuple(L)",
        1,
    ),
    (
        Row::Bounded,
        "L = [10**4000] * 10000",
        "t = ('%s' * 10000) % tuple(L)",
        1,
    ),
    (
        Row::Bounded,
        "L = (10**1500,) * 30000",
        "t = ('%d' * 30000) % L",
        1,
    ),
    (
        Row::Bounded,
        "L = [10**4000] * 10000",
        "t = ('{}' * 10000).format(*L)",
        1,
    ),
    (Row::Bounded, "L = [10**4000] * 10000", "r = repr(L)", 1),
    (Row::Bounded, "L = [10**1500] * 40000", "r = str(L)", 1),
    (
        Row::Bounded,
        "d = {i: 10**4000 for i in range(10000)}",
        "r = repr(d)",
        1,
    ),
    // The repr steps (`str(L)` above), and a lone f-string field is its
    // formatted str: no `BUILD_STRING` copy of its 40 MB (70 ms before
    // [M]).
    (Row::Bounded, "L = [10**1500] * 40000", "t = f'{L}'", 1),
    // T4: `%r`, `%a` and `%s` of a long text took the native repr (100-571
    // ms); they now take the stepped one. A bytearray is a piece of a
    // bytes result, not copied twice (56 ms). A huge precision's zeros past
    // a double's exact digits, or an int's zero fill, are a piece (88-112
    // ms; `%.60000000e` was a MemoryError): 4-13 ms [M].
    (Row::Bounded, "S = 'é' * 15_000_000", "t = '%a' % S", 1),
    (Row::Bounded, "S = 'a\\n' * 10_000_000", "t = '%r' % S", 1),
    (Row::Bounded, "S = 'a' * 30_000_000", "t = '%s' % [S]", 1),
    (Row::Bounded, "S = 'a' * 30_000_000", "t = str([S])", 1),
    // A long str or bytes inside a container takes the stepped repr too
    // (P2's row: `repr([S])` was one 106 ms step), escapes included:
    // 10-11 ms [M].
    (Row::Bounded, "S = 'a' * 30_000_000", "t = repr([S])", 1),
    // Those pieces are joined: under 32 MiB a join copied its result in
    // one step, twice (into the build buffer, then the str), 36 ms at 30
    // MB with the page faults [M]; it now steps from one step's copy.
    (
        Row::Bounded,
        "S = 'a' * 30_000_000",
        "t = ''.join(['[', S, ']'])",
        1,
    ),
    (Row::Bounded, "B = b'a' * 30_000_000", "t = repr([B])", 1),
    // The worst size for these rows: a result just under 32 MiB is first
    // touched in one step, 14,520 minor page faults and 41 ms [M,
    // performance review], where the rows above sit at 0.74-0.86 of the
    // limit. Pinned so the oracle sees the worst case.
    (Row::Bounded, "B = b'a' * 33_000_000", "t = repr([B])", 1),
    (
        Row::Bounded,
        "S = 'é\\x01' * 6_000_000",
        "t = repr((S,))",
        1,
    ),
    (Row::Bounded, "", "t = '%.50000000f' % 1.5", 1),
    (Row::Bounded, "", "t = '%.60000000e' % 1.5", 1),
    (Row::Bounded, "", "t = '%.60000000d' % 5", 1),
    (
        Row::Bounded,
        "B = bytearray(b'a') * 30_000_000",
        "t = b'%s%s' % (B, B)",
        1,
    ), // The review's T1a: `sorted(key=str)` routed a key call through a
    // safepoint only every 4,096 keys, and `print` converted every argument
    // in one step (1.07-1.08 s for 3,000 ints of 4,299 digits). A key call
    // after one that did long work now goes through a safepoint too, and
    // `print` hands off to its twin past a step's work: 2-4 ms [M].
    (
        Row::Bounded,
        "a = [10**4298 + i for i in range(3000)]",
        "b = sorted(a, key=str)",
        1,
    ),
    (
        Row::Bounded,
        "a = [10**4298 + i for i in range(3000)]",
        "print(*a)",
        1,
    ),
    // With the digit limit off, one conversion costs up to the int cap's
    // work: every budget checks a big int's cost (`Vm::decimal_units`)
    // before converting it, so a step holds one such conversion. At 40,000
    // digits, 34 ms a step [M]; at 48,000, one conversion alone is 46-52
    // ms, the int cap's (P3).
    (
        Row::Bounded,
        "import sys\nsys.set_int_max_str_digits(0)\na = [10**39999 + i for i in range(100)]",
        "b = sorted(a, key=str)",
        1,
    ),
    (
        Row::Bounded,
        "import sys\nsys.set_int_max_str_digits(0)\na = [10**39999 + i for i in range(100)]",
        "print(*a)",
        1,
    ),
    (
        Row::Bounded,
        "import sys\nsys.set_int_max_str_digits(0)\na = [10**39999 + i for i in range(100)]",
        "t = ('%d' * 100) % tuple(a)",
        1,
    ),
    (
        Row::Bounded,
        "import sys\nsys.set_int_max_str_digits(0)\na = [10**39999 + i for i in range(100)]",
        "t = repr(a)",
        1,
    ),
    (
        Row::Bounded,
        "import sys\nsys.set_int_max_str_digits(0)\na = [10**39999 + i for i in range(100)]",
        "t = ('{}' * 100).format(*a)",
        1,
    ),
    // `x in d.values()` goes through the entries in place, a step at a
    // time (`nstep::op::DV_IN`): collecting the values first was one
    // 104 ms step [M]; now 3.1 ms. A dict of 2M entries is about the
    // largest whose entries fit the 64 MiB object cap.
    (
        Row::Bounded,
        "d = {i: 0 for i in range(2_000_000)}",
        "x = -1 in d.values()",
        1,
    ),
    // A range gathered into a list or tuple: in one native step until the
    // allocation floor (8M items 155-165 ms, 4M 72-85 ms [M]). `list` and
    // `tuple` of a range now build from it a slice per step
    // (`Vm::range_seq`); a range iterator is gathered a chunk per step, the
    // rest into a list sized up front (`Vm::slow_call`).
    (Row::Bounded, "", "xs = list(range(8_000_000))", 1),
    (Row::Bounded, "", "t = tuple(range(8_000_000))", 1),
    (
        Row::Bounded,
        "it = iter(range(8_000_000))",
        "xs = list(it)",
        1,
    ),
    // A full list of 1M items or more grows a slice per step on `append`
    // and `insert` (`builtins::GROW_LONG`): an exactly sized 8M list grew
    // to the cap in one 35 ms step [M]. A comprehension's own growth
    // (`LIST_APPEND`) doubles in one step, 19-20 ms at 4M -> 8M [M].
    (
        Row::Bounded,
        "xs = list(range(8_000_000))",
        "xs.append(0)",
        1,
    ),
    (Row::Bounded, "", "ys = [i for i in range(8_388_608)]", 1),
    // An f-string whose parts pass a step's work hands them to the stepped
    // `''.join` (`Vm::reserve_build`, run by `execute`); `BUILD_STRING`'s
    // arm is unchanged. 60-71 ms in one step before [M].
    (Row::Bounded, "B = b'a' * 60_000_000", "t = f'<{B}>'", 1),
    (Row::Bounded, "s = 'a' * 60_000_000", "t = f'<{s}>'", 1),
    (Row::Bounded, "S = 'a' * 30_000_000", "t = f'{S}{S}'", 1),
    // A bytearray's repr is built in steps (74 ms before [M]), as a long
    // str's `!r` is; `format(s, '')` of a str is the str (a 40 ms copy
    // before [M]).
    (
        Row::Bounded,
        "B = bytearray(b\"a'\" * 15_000_000)",
        "t = str(B)",
        1,
    ),
    (Row::Bounded, "B = b'a' * 30_000_000", "t = f'{B!r}'", 1),
    (Row::Bounded, "s = '\\xe9' * 10_000_000", "t = f'{s!r}'", 1),
    // A bytearray grown by 1 MB 60 times: copies past 8 MiB go a step at a
    // time (`job::GROW`); the longest step is a collection freeing the old
    // blocks (14 ms; 4.7 ms with the collector off [M]).
    (
        Row::Bounded,
        "xs = [bytearray(b'a' * 1_000_000) for _ in range(60)]",
        "ba = bytearray()\nfor x in xs:\n    ba += x",
        1,
    ),
    // A copy of 64 MiB by a constructor or `copy()`, a step at a time (a
    // `job::REPEAT` of one; 34-36 ms one step before [M]).
    (Row::Bounded, "b = b'a' * 67_108_864", "c = bytearray(b)", 1),
    (
        Row::Bounded,
        "b = bytearray(b'a' * 67_108_864)",
        "c = bytes(b)",
        1,
    ),
    (
        Row::Bounded,
        "b = bytearray(b'a' * 67_108_864)",
        "c = b.copy()",
        1,
    ),
    // A strict codec error. A decode's `object` is a `bytes` of what the
    // codec read: of a `bytearray`, a copy made a step at a time
    // (`job::DECODE_ERR`) before the error is raised. An encode's `str()`
    // finds its one char from the nearer end, as `s[i]` does (6 ms [M]).
    (
        Row::Bounded,
        "b = bytearray(67_108_864)\nb[-1] = 255",
        "try:\n    b.decode()\nexcept UnicodeDecodeError as e:\n    n = e.start",
        1,
    ),
    (
        Row::Bounded,
        "s = '\\xe9' * 16_000_000 + '\\u20ac' + '\\xe9' * 16_000_000",
        "try:\n    s.encode('latin-1')\nexcept UnicodeEncodeError as e:\n    t = str(e)",
        1,
    ),
    // The fields are kept by the constructor: a read and `str()` do not
    // look at `args`, however long (one `str(e)` with 8M `args` was a
    // 285-298 ms step for 14 units on the first form [M, reviewer]). A
    // `bytearray` given is copied once, a step at a time, by `bytes()`.
    (
        Row::Bounded,
        "t = (0,) * 8_000_000\ne = UnicodeDecodeError('u', b'abc', 0, 1, 'r')\ne.args = t",
        "s = str(e)\nn = e.start\nb = e.object",
        8,
    ),
    (
        Row::Bounded,
        "b = bytearray(67_108_864)",
        "e = UnicodeDecodeError('utf-8', b, 0, 1, 'r')\no = e.object",
        1,
    ),
    // `str()` with a long `encoding` and `reason`: joined in the prelude,
    // a step at a time (30 MiB each was one step of 111-123 ms [M]).
    (
        Row::Bounded,
        "e = UnicodeEncodeError('e' * 31_457_280, 'x', 0, 1, 'r' * 31_457_280)",
        "s = str(e)",
        1,
    ),
    // A wide tab (review T6, P5): a short str whose tabs widen it past a
    // step goes the stepped way (`STR_MAP`), and a bytes tab's spaces go
    // out a MiB at a time. Before, the str was one 107-128 ms step charged
    // 60.9M units, the bytes one 27 ms step, and 240 MB of bytes tabs past
    // the object cap was charged 3.75M units in one slice before its
    // `MemoryError`; now 10.6, 13.0, 3.7 ms and at most 956,499 units
    // past a 1,000-unit slice [M].
    (Row::Bounded, "pass", "t = '\\t'.expandtabs(60_000_000)", 1),
    (
        Row::Bounded,
        "pass",
        "t = '\\xe9\\t'.expandtabs(60_000_000)",
        1,
    ),
    (
        Row::Bounded,
        "pass",
        "t = ('x' + '\\t' * 9).expandtabs(6_000_000)",
        1,
    ),
    (Row::Bounded, "pass", "t = b'\\t'.expandtabs(60_000_000)", 1),
    (
        Row::Bounded,
        "pass",
        "try:\n    bytearray(b'ab\\t' * 4).expandtabs(60_000_000)\nexcept MemoryError:\n    pass",
        1,
    ),
    // Number formats (P3, review T3 and T5). A big int's power-of-two
    // digits go into their str's block a step at a time (`Vm::pow2_text`,
    // `NUM_TEXT`), grouped and padded as asked: `format(n, 'b')` at the
    // int cap took 203-283 ms in one step, `'_x'` 89-94 ms and `'%o' % n`
    // 61 ms [M, review]; `%x %X %o` past a million digits ask `_percent`
    // for that `format`.
    (
        Row::Bounded,
        "n = (1 << 67_000_000) - 1",
        "t = format(n, 'b')",
        1,
    ),
    (Row::Bounded, "n = (1 << 67_000_000) - 1", "t = f'{n:b}'", 1),
    (
        Row::Bounded,
        "n = -((1 << 67_108_000) - 1)",
        "t = format(n, '#_x')",
        1,
    ),
    (
        Row::Bounded,
        "n = -((1 << 67_108_000) - 1)",
        "t = '%o' % n",
        1,
    ),
    (
        Row::Bounded,
        "n = -((1 << 67_108_000) - 1)",
        "t = b'%x' % n",
        1,
    ),
    // `str.format` hands such a field to its prelude twin, which calls
    // `format`.
    (
        Row::Bounded,
        "n = (1 << 67_000_000) - 1",
        "t = '{:b}'.format(n)",
        1,
    ),
    (Row::Bounded, "", "t = '{:>67000000}'.format(5)", 1),
    (
        Row::Bounded,
        "n = (1 << 20_000_000) - 1",
        "t = format(n, '>60000000x')",
        1,
    ),
    // A number padded to a width of tens of millions: its text as before,
    // then padded and zero-filled in steps (`Vm::num_pad`, `STR_FILL`):
    // 71-160 ms in one step before, and 539 ms before the `MemoryError` of
    // a width past the limit [M, P1 and review].
    (Row::Bounded, "", "t = format(5, '>60000000d')", 1),
    (Row::Bounded, "", "t = format(5, '060000000,d')", 1),
    (Row::Bounded, "", "t = f'{1.5:060000000,.2f}'", 1),
    (Row::Bounded, "", "t = format(1+2j, '>67000000')", 1),
    (
        Row::Bounded,
        "",
        "try:\n    t = format(5, '0200000000,d')\nexcept MemoryError:\n    pass",
        1,
    ),
    // Decimal text of a big int with CPython's digit limit off: one
    // conversion, refused past about 41,000 digits (`decimal_ns`, at
    // 48,000 digits 46-52 ms [M, P1]).
    (
        Row::Bounded,
        "import sys\nsys.set_int_max_str_digits(0)\nn = 10 ** 40_800 + 1",
        "t = format(n, 'd')",
        1,
    ),
    (
        Row::Bounded,
        "import sys\nsys.set_int_max_str_digits(0)\nn = 10 ** 48_000 + 1",
        "try:\n    t = str(n)\nexcept MemoryError:\n    pass",
        1,
    ),
    // Big-int division charges its quotient's work (review U1: `a / c`
    // charged 2G units, `pow(a, 2, 1 << 64)` 2 units for 41-44 ms), and
    // `/` shifts the dividend rather than a copy of the divisor. A
    // division by two limbs runs about 22 ns a quotient limb, which
    // `div_ns` took for 16: at the int cap 46-60 ms [M], now refused.
    (
        Row::Bounded,
        "a = (1 << 67_000_000) - 1\nc = (1 << 66_999_945) + 12345",
        "q = a / c",
        1,
    ),
    (
        Row::Bounded,
        "a = (1 << 67_000_000) - 1\nc = (1 << 66_999_945) + 12345",
        "q = a % c",
        1,
    ),
    (
        Row::Bounded,
        "a = (1 << 67_000_000) - 1",
        "try:\n    q = pow(a, 2, 1 << 64)\nexcept MemoryError:\n    pass",
        1,
    ),
    (
        Row::Bounded,
        "a = (1 << 67_000_000) - 1",
        "try:\n    q = a % ((1 << 40) + 1)\nexcept MemoryError:\n    pass",
        1,
    ),
    (Row::Bounded, "a = (1 << 67_000_000) - 1", "q = a // 3", 1),
    // Long needles and separators (P5): a needle of 512 KiB or more is
    // searched by the bytes search, which keeps its factorization between
    // steps, and `split`, `rsplit`, `replace` and `partition` by one go
    // through `find`/`rfind` in a prelude twin. Before, a step read a 16M
    // needle whole: 58-79 ms, `partition` one 145 ms step and `rsplit`
    // never ended (its window went up); a needle longer than a short
    // text was 58-116 ms of searcher setup; `partition` searched a long
    // text in one step, 89-109 ms with a 101-char separator. Now 1.2-14
    // ms [M]. The search's factorization is charged four bytes of work a
    // needle byte (a bytes needle of 16M: 20 ms steps at one).
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nR = 'b' + 'a' * 16_000_000",
        "i = S.rfind(R)",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nN = 'a' * 16_000_000 + 'b'",
        "t = N in S",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nN = 'a' * 16_000_000 + 'b'",
        "n = S.count(N)",
        1,
    ),
    (
        Row::Bounded,
        "S = '\\xe9' * 33_000_000\nN = '\\xe9' * 8_000_000 + 'b'",
        "i = S.find(N, 1000, 30_000_000)",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nN = 'a' * 16_000_000 + 'b'",
        "t = S.split(N)",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nR = 'b' + 'a' * 16_000_000",
        "t = S.rsplit(R)",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nN = 'a' * 16_000_000 + 'b'",
        "t = S.replace(N, 'x')",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000\nR = 'b' + 'a' * 16_000_000",
        "t = S.rpartition(R)",
        1,
    ),
    (
        Row::Bounded,
        "N = 'a' * 16_000_000 + 'b'",
        "t = 'abc'.replace(N, 'x'), 'abc'.rsplit(N), 'abc'.find(N)",
        1,
    ),
    (
        Row::Bounded,
        "S = 'a' * 67_000_000",
        "t = S.partition('a' * 100 + 'b')",
        1,
    ),
    (
        Row::Bounded,
        "b = b'a' * 67_000_000\nN = b'a' * 16_000_000 + b'b'",
        "i = b.find(N)",
        1,
    ),
    // A separator just under the long-needle cut (P5, review): each
    // search built std's two-way searcher over the whole needle again,
    // charged nothing for it, so adjacent matches made `split` one 275 ms
    // step and `replace` 266 ms; below the stepped paths' cut, a native
    // `replace` built one a match, 117 ms. Searches now charge the
    // needle's setup and the natives build one searcher: 14-19 ms [M].
    (
        Row::Bounded,
        "P = 'a' * 524_287\nV = 'a' * 60_000_000",
        "t = V.split(P)",
        1,
    ),
    (
        Row::Bounded,
        "P = 'a' * 524_287\nV = 'a' * 60_000_000",
        "t = V.replace(P, 'x')",
        1,
    ),
    (
        Row::Bounded,
        "P = 'a' * 400_000\nV = 'a' * 16_000_000",
        "t = V.replace(P, 'x')",
        1,
    ),
    (
        Row::Bounded,
        "P = 'a' * 400_000\nV = 'a' * 16_000_000",
        "t = V.count(P, 1)",
        1,
    ),
    // A 9-byte needle (std's two-way searcher's range) with adjacent
    // matches: each search read the rest of the text as UTF-8 again, so
    // `count` of 3 MB was one 10.9 s step and `split` 16 s [M]. Searches
    // now read pieces that double from the needle's size: 9-30 ms [M].
    (
        Row::Bounded,
        "P = 'a' * 9\nV = 'a' * 3_000_000",
        "t = V.count(P)",
        1,
    ),
    (
        Row::Bounded,
        "P = 'a' * 9\nV = 'a' * 3_000_000",
        "t = V.split(P)",
        1,
    ),
    // Its last steps reversed the 6.7M pieces an item at a time: 25 to 41
    // ms a step on musl, 13 to 28 on glibc; swapped in place in one pass,
    // 3 to 7 [M]. With mimalloc the step that doubles the list from 4M to
    // 8M items (a new 64 MiB buffer, 32 MiB copied) reads 20 to 22 ms or 45
    // to 47 when its pages are new, against limits of 38 to 56 (CHANGELOG,
    // Known issues).
    (
        Row::OpenOnMimalloc,
        "P = 'a' * 9\nV = 'a' * 60_000_000",
        "t = V.rsplit(P)",
        1,
    ),
    // `copy.copy` and `copy.deepcopy` of a bytearray are its `copy()`, a
    // step at a time past 8 MiB (they raised TypeError before; 4.5 ms [M]).
    (
        Row::Bounded,
        "import copy\nb = bytearray(b'a' * 67_108_864)",
        "c = copy.copy(b)",
        1,
    ),
    (
        Row::Bounded,
        "import copy\nb = bytearray(b'a' * 67_108_864)",
        "c = copy.deepcopy([b, b])",
        1,
    ),
    // Keys with equal hashes (every `k * M` hashes to 0). A set insert
    // compared each one for free (`set_put`): one 7.7 s step for `set(xs)`;
    // and the table rebuilds re-probed all those placed before each key
    // uncharged, in one step: 191 ms for the `add` loop's, 80 ms and 71 ms
    // for the dict's, on d7c2b437 [M]. Now an insert
    // has one walk, and a rebuild charges its probes and ends a slice on
    // them: 29 ms, 5 ms, 27 ms and 2 ms [M].
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = [k * M for k in range(1, 10_001)]",
        "s = set(xs)",
        1,
    ),
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = [k * M for k in range(1, 10_001)]",
        "s = set()\nfor x in xs: s.add(x)",
        1,
    ),
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = [k * M for k in range(1, 10_001)]",
        "d = dict.fromkeys(xs)",
        1,
    ),
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = [k * M for k in range(1, 10_001)]",
        "d = {}\nfor x in xs: d[x] = None",
        1,
    ),
    // Past 16,384 colliding keys a probe runs out of its walk and the
    // prelude twin inserts (`_dset`), whose `_dict_insert` rebuilt a full
    // table in one step: 1.2-1.3 s for the loop and 1.2-1.4 s for
    // `dict.fromkeys` at 33,500 keys on e54f65f [M, this oracle on shipped
    // codegen, a host at 1.4-1.7 times the reference] (at 17,000 the loop
    // meets no full table on the twin's path). It now rebuilds in steps,
    // then inserts: 5.0-7.3 ms and 15.8-26.0 ms [M].
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = [k * M for k in range(1, 33_501)]",
        "d = {}\nfor x in xs: d[x] = None",
        1,
    ),
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = [k * M for k in range(1, 33_501)]",
        "d = dict.fromkeys(xs)",
        1,
    ),
    // Set lookups that each walk their 16 free runs (uncharged) in a bulk
    // operation's step: 151-175 ms at 2^22 slots on 71b9c5b5 [M]. A step
    // now counts each item's free runs: 8-11 ms [M].
    (Row::Bounded, SET_FREE_RUNS, "r = s.isdisjoint(big)", 1),
    (Row::Bounded, SET_FREE_RUNS, "s.update(big)", 1),
    // A set table's rebuild gave each key 16 free runs, counted nowhere,
    // so keys whose probes share their first 8 to 16 runs in the new
    // table rebuilt 2^18 slots in one step: 76.0-76.9 ms on a70cac6
    // against 18.2-21.3 ms on e54f65f [M, this oracle on shipped codegen,
    // limits 75.0-100.0 ms; plain release: 192-196 against 43-46 ms]. Each
    // key's free runs now count toward where a rebuild slice ends:
    // 13.4-13.7 ms [M].
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = []\nfor sg in range(16):\n    c = (sg * 0x9E3779B97F4A7C15 + 12345) & ((1 << 54) - 1)\n    for g in range(128):\n        h = c + (g << 54)\n        if h >= M: continue\n        for j in range(80): xs.append(h + j * M)",
        "s = set()\nfor x in xs: s.add(x)",
        1,
    ),
    // A dict's growth in one step counted no free slot an entry walked,
    // so a table of 2^20 entries, 32 to a hash and inserted round-robin,
    // that grew beside a 46 MiB bytearray (too little memory left to grow
    // in steps) walked every chain in one step (H1): 140-142 ms on
    // e5c2af1 against 50.4-51.2 ms on e54f65f [M, this oracle's `gaps` on
    // shipped codegen, one core]. It now ends on the free slots walked,
    // as e54f65f's did: 46.6-47.2 ms [M]. The largest growth below
    // GROW_STEPS (262,143 entries, one deleted) doubled too, 20.9-21.1 ms
    // against 11.2-11.4 ms, but stays under the limit either way, so it
    // has no row: a row here guards only the limit. The growth moves all
    // 2^20 entries in one step, bound by memory latency: 35 to 81 ms
    // against limits of 50 to 70, over in 14 of 72 runs on glibc, musl's
    // `malloc` and mimalloc [M, 24 alternated runs each]. `Open`
    // (CHANGELOG, Known issues) until a growth with memory tight goes on
    // in steps too.
    (
        Row::Open,
        "M = (1 << 61) - 1\nxs = [k * M + c for k in range(32) for c in range(1, 32_770)][:(1 << 20) + 1]\nballast = bytearray(46 << 20)",
        "d = {}\nfor x in xs: d[x] = None",
        1,
    ),
    // Lookups past deleted slots and colliding keys counted no slots, so
    // a native loop of them was one step: 1,517 ms for `s.isdisjoint(big)`
    // and 344 ms for the dict's keys with all but one key deleted, 869 ms
    // and 1,412 ms for items views at 5,000 colliding keys; a big int
    // copied before `inf` or `nan` was looked at, 363-447 ms; a set grown
    // by the prelude's inserts (a generator) rebuilt its table in one
    // step, 61 ms and 84 ms; on 3feacaa1, plain release [M]. A probe now
    // spends a visit per run of slots and ends a slice on them, `nan` and
    // the infinities decide without a copy, and an insert of a natively
    // hashed key grows a big table in steps.
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = [k * M for k in range(1, 10_001)]\nbig = [k * M for k in range(20_001, 40_001)]\ns = set(xs)\nfor x in xs[:-1]: s.discard(x)",
        "r = s.isdisjoint(big)",
        1,
    ),
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = [k * M for k in range(1, 10_001)]\nbig = [k * M for k in range(20_001, 40_001)]\nd = dict.fromkeys(xs)\nfor x in xs[:-1]: del d[x]",
        "r = d.keys().isdisjoint(big)",
        1,
    ),
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = [k * M for k in range(1, 5_001)]\nd = dict.fromkeys(xs)\nd3 = dict.fromkeys(xs[::2])",
        "r = d.items() & d3.items()",
        1,
    ),
    (
        Row::Bounded,
        "M = (1 << 61) - 1\nxs = [k * M for k in range(1, 5_001)]\nd = dict.fromkeys(xs)\nd2 = dict.fromkeys([k * M for k in range(5_001, 10_001)])",
        "r = d.items().isdisjoint(d2.items())",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**20 - 1)) | 0x1234567\nzs = [c] * 4000\ninf = float('inf')\nnan = float('nan')",
        "r = inf in zs",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**20 - 1)) | 0x1234567\nzs = [c] * 4000\ninf = float('inf')\nnan = float('nan')",
        "r = zs.count(nan)",
        1,
    ),
    (
        Row::Bounded,
        "d = dict(zip(range(1_000_000), range(1_000_000)))\ndef gen():\n    for i in range(1_000_000, 2_000_000):\n        yield (i, i)",
        "r = d.items() | gen()",
        1,
    ),
    (
        Row::Bounded,
        "x = 0",
        "r = set(i for i in range(3_000_000))",
        1,
    ),
    // A native probe's charge stopped at its walk's budget, 16,383 units,
    // so a native loop of lookups each past 131,104 slots was one step:
    // 97.9 ms for `dd.keys() & zs` and 80.4 ms for `isdisjoint` with
    // 2,097,144 int keys on the probe path of 'zz', the most the default
    // limit holds, on 11110028 [M, correctness review]. The visits past it
    // are now charged too, and a step held one lookup: 49.9 and 53.0 ms on
    // a 54.7 ms limit on an idle machine [M, lane G] (AE1). A lookup now
    // goes on a slice of `WALK` visits per step.
    (
        Row::Bounded,
        "mask = (1 << 22) - 1\ndef path(q, n):\n    q &= (1 << 64) - 1\n    i = (q ^ (q >> 32)) & mask\n    p, seen, out = q, set(), []\n    while len(out) < n:\n        if i not in seen:\n            seen.add(i)\n            out.append(i)\n        p >>= 5\n        i = (5 * i + 1 + p) & mask\n    return out\ndd = dict.fromkeys(path(hash('zz'), 2_097_144))\nzs = ['zz'] * 100",
        "r = dd.keys() & zs",
        1,
    ),
    (
        Row::Bounded,
        "mask = (1 << 22) - 1\ndef path(q, n):\n    q &= (1 << 64) - 1\n    i = (q ^ (q >> 32)) & mask\n    p, seen, out = q, set(), []\n    while len(out) < n:\n        if i not in seen:\n            seen.add(i)\n            out.append(i)\n        p >>= 5\n        i = (5 * i + 1 + p) & mask\n    return out\ndd = dict.fromkeys(path(hash('zz'), 2_097_144))\nzs = ['zz'] * 100",
        "r = dd.keys().isdisjoint(zs)",
        1,
    ),
    // Big ints and long texts: a compare or hash counted one visit
    // whatever the operands' size, and copied each big int (`big_of`). On
    // 9a80bbea [M]: 4,000 references to one 2^20-bit int took 882-889 ms
    // in one step for `min`, `count`, `in` and `sorted`, 1,124 ms for the
    // tuple's hash; 17 ints of 2^26 bits 207-313 ms for `max`, `index`,
    // `sorted` and the tuple's hash; 200 references to one 8 MB str 71 ms
    // for `sorted`, a long sort's runs of 8 KiB ints 200 ms. Now ints
    // compare and hash in place, a unit and visit per 64 bytes compared
    // (32 hashed), a tuple's hash hands off once they spend its walk, and
    // a sort past `SORT_CAP` visits in one go leaves the rest to
    // `_sort_keys`: at most 11 ms [M].
    (
        Row::Bounded,
        "c = (1 << (2**20 - 1)) | 0x1234567\nzs = [c] * 4000",
        "m = min(zs)",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**20 - 1)) | 0x1234567\nzs = [c] * 4000",
        "n = zs.count(c + 0)",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**20 - 1)) | 0x1234567\nzs = [c] * 4000",
        "b = (c + 1) in zs",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**20 - 1)) | 0x1234567\nzs = [c] * 4000",
        "ys = sorted(zs)",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**20 - 1)) | 0x1234567\nzs = [c] * 4000",
        "h = hash(tuple(zs))",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**26 - 1)) | 5\nd = c + 2\nws = [c + 0 for _ in range(16)] + [d]",
        "m = max(ws)",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**26 - 1)) | 5\nd = c + 2\nws = [c + 0 for _ in range(16)] + [d]",
        "i = ws.index(d)",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**26 - 1)) | 5\nd = c + 2\nws = [c + 0 for _ in range(16)] + [d]",
        "ys = sorted(ws)",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**26 - 1)) | 5\nd = c + 2\nws = [c + 0 for _ in range(16)] + [d]",
        "h = hash(tuple(ws))",
        1,
    ),
    (
        Row::Bounded,
        "c = (1 << (2**26 - 1)) | 5\nd = c + 2\nws = [c + 0 for _ in range(16)] + [d]",
        "h = hash(frozenset(ws))",
        1,
    ),
    (
        Row::Bounded,
        "t = 'x' * (1 << 23)\nts = [t] * 200",
        "ys = sorted(ts)",
        1,
    ),
    (
        Row::Bounded,
        "c = 1 << (2**16 - 1)\nys = [c + 0 for _ in range(12_000)]",
        "zs = sorted(ys)",
        1,
    ),
    // `re`: a slice's budget grows with the backtrack stack (four steps a
    // word), so a capture under a star made one slice of the whole match:
    // 337-355 ms and 593-605 ms on fe8b1c2 at 1.59 of the reference's time [M].
    // Possessive and atomic repeats now drop each iteration's frames, so
    // the stack stays small and a slice is `SLICE` steps.
    (
        Row::Bounded,
        "import re\ns = 'ab' * 600_000\np = re.compile(r'(a|b)*+c')",
        "m = p.match(s)",
        1,
    ),
    (
        Row::Bounded,
        "import re\ns = 'ab' * 600_000\np = re.compile(r'(?>(?>(?>(a|b)*)))c')",
        "m = p.match(s)",
        1,
    ),
    // A class with many bases: its C3 merge scans the lists for each class
    // it takes (20,000 bases were one step of 6.1 s [M]).
    (
        Row::Bounded,
        "bs = tuple([type('B', (Exception,), {}) for i in range(20_000)])",
        "W = type('W', bs, {})",
        1,
    ),
    // Suspended generators of one function at two yields in turn, in a
    // 100-register call window, under 32 MiB live, so each collection is
    // a full one in one step, and its pass over the generator registry
    // clears each copy suspended since the last (`Vm::clear_suspended`).
    // The row alone, shipped, one run each at load under 3: the parent
    // 6.4 ms, this pass 10.0 ms, limit 36 ms [M]; layer 2's mark visit
    // read 28.1-30.6 ms against the parent's 13.3-15.4 on its reviewer's
    // machine [S].
    (
        Row::Bounded,
        SETUP_W100,
        "junk = [str(i) * 20 for i in range(8000)]",
        60,
    ),
    // Starting each task and blocking on f needs no guest call or loop.
    // Check every task's outcome: before in-place waiters, the program
    // completed while most of its tasks failed registration (MemoryError).
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()\nf = loop.create_future()\nasync def w():\n    await f",
        "ts = [asyncio.create_task(w()) for _ in range(40_000)]\nawait asyncio.sleep(0)\nf.set_result(1)\nawait asyncio.sleep(0)\nassert all(t.result() is None for t in ts)",
        1,
    ),
    // Loop callbacks with no call or loop in their body: the scheduler
    // starts each one, and the start is a safepoint, as `_aio_callback`'s
    // `CALL_EX` was. Entered without one, 300,000 done callbacks were one
    // step of 203 ms, and 300,000 `call_soon` callbacks one of 240 ms
    // [M, reviewer, af35804].
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()\nran = 0\ndef cb(x):\n    global ran\n    ran += 1\nfs = [loop.create_future() for _ in range(6000)]\nfor f in fs:\n    for _ in range(50):\n        f.add_done_callback(cb)",
        "for f in fs:\n    f.set_result(1)\nawait asyncio.sleep(0)",
        1,
    ),
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()\nran = 0\ndef cb():\n    global ran\n    ran += 1",
        "for _ in range(300_000):\n    loop.call_soon(cb)\nawait asyncio.sleep(0)",
        1,
    ),
    // Cancelled handles: the scheduler skips each one at a safepoint of its
    // own. Skipped with none, 1,000,000 were one step of 63-66 ms
    // [M, reviewer, shipped ee7edb6]; the row's worst step is now the
    // loop that builds the queue, and a skip is about 0.05 ms.
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()\ndef cb():\n    pass",
        "for _ in range(1_000_000):\n    loop.call_soon(cb).cancel()\nawait asyncio.sleep(0)",
        1,
    ),
    // A cancel walks its tree in steps of bounded work
    // (`nstep::op::CANCEL`): in one step, the cancel of a gather of 1.2M
    // futures took 87 to 187 ms [M, shipped, 4a3b195].
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()\nfs = [loop.create_future() for _ in range(1_200_000)]\nG = asyncio.gather(*fs)",
        "G.cancel()",
        1,
    ),
    // A settle that wakes a task scans the blocked tasks, so a step's
    // budget is work, not visits: the cancel of 30,000 sleeping tasks was
    // one step of 1.66 to 1.88 s, and of 40,000 tasks each waiting on a
    // future one of 113 to 118 ms [M, shipped, 4a3b195]. The second
    // gathers its tasks last first, so each wake looks through the whole
    // list of blocked tasks.
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nasync def w():\n    await asyncio.sleep(1000)",
        "ts = [asyncio.create_task(w()) for _ in range(30_000)]\nawait asyncio.sleep(0)\nG = asyncio.gather(*ts)\nG.cancel()",
        1,
    ),
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()\nasync def w(f):\n    await f",
        "ts = [asyncio.create_task(w(loop.create_future())) for _ in range(40_000)]\nawait asyncio.sleep(0)\nG = asyncio.gather(*ts[::-1])\nG.cancel()",
        1,
    ),
    // Each gather that listens to a settled future is looked up among the
    // tracked ones (those of the running cell): 100,000 gathers of a
    // future each, cancelled through the gather over them, were one step
    // of 1,351 ms [M, reviewer, shipped 17a2daf]. 60,000 here: the steps
    // are bounded, their sum (0.7 s) is not shorter.
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()",
        "G = asyncio.gather(*[asyncio.gather(loop.create_future()) for _ in range(60_000)])\nG.cancel()",
        1,
    ),
    // A visit reads the waiters of the future it reaches, every time it
    // reaches it: one with 10,000 (gathers that settled since, which
    // nothing has dropped from it yet) under a gather that names it
    // 4,000 times, all in one step (0.35 s) were a visit a unit.
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()\nf = loop.create_future()\nbad = loop.create_future()\nbad.set_exception(ValueError())\nG = asyncio.gather(*[f] * 4_000)\ngs = [asyncio.gather(f, bad) for _ in range(10_000)]\nawait asyncio.sleep(0)\nn = sum(g.done() for g in gs)",
        "G.cancel()",
        1,
    ),
    // A task that must raise the cancel itself hands the message to the
    // gathers that wait on it, one read of its waiters at every visit
    // (`Vm::set_must_cancel`): a task with 20,000 done callbacks under a
    // gather that names it 20,000 times was one step 787 to 836 ms past
    // the deadline with a task's visit a unit [M, shipped].
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nasync def w():\n    await asyncio.sleep(1000)\ndef cb(f):\n    pass",
        "t = asyncio.create_task(w())\nfor _ in range(20_000):\n    t.add_done_callback(cb)\nG = asyncio.gather(*[t] * 20_000)\nG.cancel('m')\nawait asyncio.sleep(0)",
        1,
    ),
    // The scheduler's own cancel, for a task that blocks with a cancel
    // pending, goes on in the same steps between tasks: on a gather of
    // 1.2M futures it was one step of 91 to 104 ms [M, shipped, 4a3b195].
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()\nfs = [loop.create_future() for _ in range(1_200_000)]\nG = asyncio.gather(*fs)\nasync def w():\n    me.cancel()\n    await G",
        "me = asyncio.create_task(w())\nawait asyncio.sleep(0)",
        1,
    ),
    // The async generators `asyncio.run` closes when it ends, 256 at a
    // time. Closed by one `gather`, 200,000 abandoned ones were one step 61
    // to 98 ms past the deadline (the gather starting their tasks), and
    // 400,000 ended the run with `MemoryError` [M, reviewer and here,
    // release builds].
    (
        Row::Bounded,
        "import asyncio\nasync def gen():\n    try:\n        yield 1\n        yield 2\n    finally:\n        pass\nasync def main():\n    for _ in range(200_000):\n        async for x in gen():\n            break",
        "asyncio.run(main())",
        1,
    ),
    // The same with 1,600,000: the two native walks of the tracked list
    // (`track_asyncgen`'s prune each time its length reaches a power of
    // two, `take_asyncgens` at the run's end) visit every tracked
    // generator in one step: 52 to 67 ms past the deadline in 4 of 4 runs,
    // and 50 to 51 ms at 800,000 in 2 of 4 [M, shipped codegen], so the
    // row is `Open` (CHANGELOG, Known issues).
    (
        Row::Open,
        "import asyncio\nasync def gen():\n    try:\n        yield 1\n        yield 2\n    finally:\n        pass\nasync def main():\n    for _ in range(1_600_000):\n        async for x in gen():\n            break",
        "asyncio.run(main())",
        1,
    ),
    // The scheduler's own steps over its blocked tasks and tracked
    // gathers. Sleeps due together woke each with a search of the blocked
    // list (30,000: one step of 141.9 ms); a settled future looked each
    // gather that listens to it up among the tracked ones (20,000: 89.8 ms;
    // 30,000 here); and a loop with nothing to run walked a gather's
    // children with a list of the gathers it had seen (40,000 futures:
    // 233.0 ms) [M, shipped, c01b09a]. A task and a gather now know their
    // places, and the walk marks the gathers it reaches.
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nasync def w():\n    await asyncio.sleep(1)",
        "ts = [asyncio.create_task(w()) for _ in range(30_000)]\nawait asyncio.sleep(0)\nawait asyncio.sleep(2)",
        1,
    ),
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()",
        "f = loop.create_future()\ngs = [asyncio.gather(f) for _ in range(30_000)]\nf.set_result(1)\nawait asyncio.sleep(0)",
        1,
    ),
    (
        Row::Bounded,
        "import asyncio\nawait asyncio.sleep(0)\nloop = asyncio.get_running_loop()",
        "G = asyncio.gather(*[loop.create_future() for _ in range(40_000)])\ntry:\n    await G\nexcept RuntimeError:\n    pass",
        1,
    ),
    // One item of a guest-sized tuple, read without copying the tuple: an
    // exception's fields from `args` (an `OSError`'s, a `SyntaxError`'s
    // from its 8M-item location tuple: 73 and 563 ms a read on e54f65f
    // [M]), `args` assigned a tuple (117 ms), a function's last
    // `__defaults__` (85 ms a call), and `isinstance` against a tuple
    // whose first item matches (2.0 s [M]).
    (
        Row::Bounded,
        "t = (0,) * 8_000_000\ne = OSError(1, 'x')",
        "e.args = t\nn = e.errno\nn = SyntaxError('m', t).filename",
        8,
    ),
    (
        Row::Bounded,
        "t = (0,) * 8_000_000\ndef f(a=1, *, b=2): return a\nf.__defaults__ = t",
        "n = f(b=3)",
        8,
    ),
    (
        Row::Bounded,
        "t = (int,) * 8_000_000",
        "b = isinstance(1, t)",
        1,
    ),
    // `except` with a tuple longer than a native walk: every item checked,
    // then matched, in the prelude (one native step at 8M items was
    // 236-272 ms [M, reviewer]).
    (
        Row::Bounded,
        "t = (KeyError,) * 8_000_000 + (ValueError,)",
        "try:\n    raise ValueError('v')\nexcept t:\n    n = 1",
        1,
    ),
    // The match visits the raised class's MRO for each item: a class 3,000
    // deep against a walk's worth of items was one step of 111 ms [M].
    (
        Row::Bounded,
        "C = Exception\nfor i in range(3000):\n    C = type('C', (C,), {})\nt = (KeyError,) * 16384",
        "try:\n    raise C()\nexcept t:\n    n = 1\nexcept Exception:\n    n = 2",
        1,
    ),
    // `isinstance` and `issubclass` charge the MRO each class item walks:
    // the same class against the same tuple was one step of 164 ms [M].
    (
        Row::Bounded,
        "C = Exception\nfor i in range(3000):\n    C = type('C', (C,), {})\nx = C()\nt = (KeyError,) * 16383",
        "b = isinstance(x, t) or issubclass(C, t)",
        1,
    ),
    // An item's metaclass MRO is looked up for `__instancecheck__`: a
    // metaclass chain 2,000 deep against 16,383 items was one step of
    // 2.3 s [M].
    (
        Row::Bounded,
        "M = type\nfor i in range(2000):\n    M = type('M', (M,), {})\nK = M('K', (), {})\nt = (K,) * 16383",
        "b = isinstance(1, t) or issubclass(int, t)",
        1,
    ),
    // A class whose MRO passes a whole walk: an item costs at most a walk,
    // the one MRO scan its check is, so a fresh walk always pays it (it
    // went back and forth with the prelude until `RecursionError`).
    (
        Row::Bounded,
        "def chain(n):\n    c = object\n    for i in range(n):\n        c = type('K', (c,), {})\n    return c\nBig = type('Big', tuple(chain(128) for k in range(128)), {})\nt = (int,) * 64 + (Big,)",
        "b = issubclass(Big, t) and isinstance(Big(), t)",
        1,
    ),
    // A long TypeError formatted through `traceback`: the "Did you forget
    // 'await'?" check reads no message past 256 bytes. Unbounded, it scanned
    // all 32 MB twice in one step: 177-184 ms past a 50 ms bound, shipped
    // `mm run` [M]. The row's 16 MB keeps an unbounded scan past the bound
    // while the formatting itself stays well under it: at 32 MB the
    // formatting alone read 49.7-55.1 ms against 50.5-56.0 ms limits, on
    // main as with the check [M]; at 60 MB it passes the bound, as it did
    // before the hint [M].
    (
        Row::Bounded,
        "import traceback\ne = TypeError('x' * 16_000_000)",
        "l = traceback.format_exception_only(e)",
        1,
    ),
    // A chain of 64 TypeErrors at the check's 256-byte cap, each the
    // costliest for it: the report renders 64 exceptions before it looks
    // at the deadline. At a 1 KiB cap, 64 of them ran 53.6-57.7 ms past a
    // 50 ms bound, shipped `mm run` [S, reviewer, 931c23c].
    (
        Row::Bounded,
        "import traceback\nm = 'descriptor ' + ' Future' * 35\ne = None\nfor i in range(64):\n    try:\n        raise TypeError(m) from e\n    except TypeError as x:\n        e = x",
        "l = traceback.format_exception(e)",
        1,
    ),
    // The longest walk of shallow items, one visit each: 16,383 classes
    // whose metaclass MRO is 15 entries, with `int`'s 1 the most an item
    // may scan for one visit (`SHALLOW`, 16); `meta_check` and then
    // `any_class` each scan every item's metaclass MRO. 18.0 to 18.2 ms
    // on shipped codegen [M].
    (
        Row::Bounded,
        "M = type\nfor _ in range(13):\n    M = type('M', (M,), {})\nKs = tuple(M('K%d' % i, (), {}) for i in range(64))\nt = Ks * 255 + Ks[:63]",
        "b = isinstance(1, t)",
        1,
    ),
    // The intern table (mm-core `string.rs`), with names a guest picks by
    // `hash()` so that they share one run of slots. Dropped, 36,000 of
    // them were pruned one by one, each removal shifting the rest of the
    // run back: 1.67 s in the collection's last step; the run is now laid
    // out again once, sorted [M, `--release`]. (No entry lies more than
    // `INTERN_WALK_CAP` slots past its hash's now, so these names double
    // the table as they are interned, until they part.)
    (
        Row::Bounded,
        intern_cluster!("36_000", "131071", "32768"),
        "names = probes = s = None\njunk = [[i] for i in range(200_000)]\njunk = None\njunk = [[i] for i in range(200_000)]",
        1,
    ),
    // The table doubling over such a run: every entry probed for its slot
    // past the ones placed before it, 235 ms; the entries are now sorted
    // first and placed with no probe [M, `--release`].
    (
        Row::Bounded,
        INTERN_GROWS,
        "for s in names:\n    hasattr(o, s)",
        1,
    ),
    // Lookups along a run of 72,000, each of which walked all of it, on
    // every path to the table: computed attribute names (125 of them ran
    // between two looks at the clock, 88 ms), an import's name (210 ms),
    // a cell's literals as it is loaded (2,000 of them: 1.2 s, with no
    // safepoint) and one-char strs, which every str constructor shares
    // (`list(text)` of 20,000: 9.9 s in one step; a loop over them,
    // 177 ms) [M, `--release`]. A lookup now reads at most
    // `INTERN_WALK_CAP` slots and one more, and no string of another hash
    // on the way.
    (
        Row::Bounded,
        intern_cluster!("72_000", "262143", "65536"),
        "for s in probes:\n    hasattr(o, s)",
        1,
    ),
    (
        Row::Bounded,
        intern_cluster!("72_000", "262143", "65536"),
        "for s in probes:\n    try:\n        __import__(s)\n    except ImportError:\n        pass",
        1,
    ),
    (
        Row::Bounded,
        intern_cluster!("72_000", "262143", "65536"),
        include_str!("steps/intern_load.py"),
        1,
    ),
    (Row::Bounded, INTERN_CHAR, "xs = list(text)", 1),
    (Row::Bounded, INTERN_CHAR, "for ch in text:\n    pass", 1),
    // Strings as far from their slots as the table lets them lie, 100
    // slots or more, where a native makes one per item with no look at
    // the clock between: a two-char str (shared like one-char ones, but
    // found in the table), which `json.loads` makes 300,000 times (61 ms
    // a step when each slot on the way meant reading a string, 6 ms with
    // the str at its own slot [M, `--release`]); and an engine's own name
    // (`len.__name__` interns `'len'` at each access).
    (
        Row::Bounded,
        intern_crowd!(
            "n = 89\n",
            "behind(crowd(hash(str(n)), 110))\n",
            "text = '[' + '\"89\",' * 300_000 + '\"89\"]'\n",
            "import json\n"
        ),
        "ys = json.loads(text)",
        1,
    ),
    (
        Row::Bounded,
        intern_crowd!("behind(crowd(hash('len'), 110))\n", "f = len\n"),
        "for i in range(200_000):\n    f.__name__",
        1,
    ),
    // Names crowded on one place of every table: the insert that finds
    // no slot within the cap grows the table from 2,048 slots to its
    // largest size in one pass (15 to 17 ms; a doubling at a time, 53
    // ms), and a later one is refused (`MemoryError`) [M, shipped].
    (
        Row::Bounded,
        intern_crowd_every_table!(),
        "try:\n    for s in names:\n        hasattr(o, s)\nexcept MemoryError:\n    pass",
        1,
    ),
    // The same over a table that holds 100,000 names: the growth moves
    // them all to the largest table in one step, 54 to 94 ms for
    // 100,000 to 1,000,000 names [M, shipped]. `Open` (CHANGELOG, Known
    // issues) with the rows below.
    (
        Row::Open,
        intern_crowd_every_table!(
            "others = ['q' + str(i) for i in range(100_000)]\n",
            "for s in others:\n",
            "    hasattr(o, s)\n"
        ),
        "try:\n    for s in names:\n        hasattr(o, s)\nexcept MemoryError:\n    pass",
        1,
    ),
    // A growth sizes the table for the name that found no slot, so
    // names that part from the crowd one table size at a time force a
    // growth each, up to one for every table size: 128 names whose
    // hashes agree in 22 bits and 13 that agree with them in 9 to 21,
    // after 30,000 other names, grow the table five times, to its
    // largest. As the literals of one cell, which is loaded with no
    // safepoint, that is one step of 86 to 120 ms; as a `hasattr` loop,
    // 80 to 142 ms between two safepoints [M, shipped]. `Open`.
    (
        Row::Open,
        "class O:\n    pass\no = O()\nothers = ['q' + str(i) for i in range(30_000)]\nfor s in others:\n    hasattr(o, s)",
        include_str!("steps/intern_chain.py"),
        1,
    ),
    // The table of names of any bucket: a doubling sorts each run of
    // entries and moves them, one step linear in the table, past the
    // bound from the doubling at 524,288 names on (to 2,097,152 slots:
    // 56 to 75 ms, 54 to 77 before; 26 to 31 ms for the one at 262,144
    // names), and 73 to 111 ms for the last one, at 1,048,576 names.
    // The prune of the 4,194,304 slots once 2,000,000 names are dropped
    // is one step too (47 to 57 ms; 253 ms by shifts) [M, shipped].
    // `Open` (CHANGELOG, Known issues) until the table grows and is
    // pruned a slice at a time.
    (
        Row::Open,
        "class O:\n    pass\no = O()\nnames = ['q' + str(i) for i in range(2_000_000)]",
        "for s in names:\n    hasattr(o, s)",
        1,
    ),
    (
        Row::Open,
        "class O:\n    pass\no = O()\nnames = ['q' + str(i) for i in range(2_000_000)]\nfor s in names:\n    hasattr(o, s)",
        "names = s = None\njunk = [[i] for i in range(400_000)]\njunk = None\njunk = [[i] for i in range(400_000)]\njunk = None\njunk = [[i] for i in range(400_000)]",
        1,
    ),
    // Slice assignment must convert a long list in steps.
    (
        Row::Bounded,
        "xs = [65] * 8_388_608; b = bytearray(b'xyz')",
        "b[1:2] = xs\nassert len(b) == len(xs) + 2 and b[:2] == b'xA' and b[-2:] == b'Az'",
        1,
    ),
];

/// How many more times a bounded row over the limit runs. This host has
/// slow phases of 1.6 to 2 times that outlast three runs taken back to
/// back [M, 300 runs of one row]: the least of 3 at once failed 5 of 100
/// at 2.0 times headroom and 22 of 100 at 1.8, the least of 6 none of 50
/// at 2.0, the least of 9 none of 33 at 1.8.
const RETRIES: usize = 6;

/// A timed row: its index in its table, its setup and program, and its
/// verdict so far: the run with the least ratio of its longest step to the
/// limit it was judged against, and every retry as (step, limit).
struct Timed<'a> {
    row: Row,
    index: usize,
    setup: &'a str,
    src: String,
    shown: String,
    worst: Duration,
    parks: usize,
    limit: Duration,
    tries: Vec<(Duration, Duration)>,
}

impl<'a> Timed<'a> {
    fn new(
        engine: &Engine,
        (index, row, setup, src, shown): (usize, Row, &'a str, String, String),
    ) -> Self {
        let (worst, parks, marked) = gaps(engine, setup, &src);
        // Near the limit, or it no longer tests a held mark.
        assert!(
            src.trim_end() != HELD_MARK_ROW || marked > 0,
            "{src}: no park held a mark"
        );
        Timed {
            row,
            index,
            setup,
            src,
            shown,
            worst,
            parks,
            limit: Duration::ZERO,
            tries: Vec::new(),
        }
    }

    fn over(&self) -> bool {
        self.row != Row::Open && self.worst > self.limit
    }
}

/// Runs each bounded row over its limit again, once a round for up to
/// [`RETRIES`] rounds, after every row has run, so the retries spread over
/// time instead of landing in one slow phase. A probe ([`speed`]) runs
/// between every two retries; each retry is judged against the limit at
/// the faster of the probes on either side of it, as the first runs are
/// against the faster of the probes before and after them all (a load
/// spike can only slow a probe). A row passes if any run is under its
/// limit. A retry runs only after a probe at or under [`SLOWEST`]
/// ([`settled`]), so each one is judged; once `waited` spends the
/// patience with the machine still past it, the retries stop and this
/// returns that probe's ratio: the rows still over are not judged.
fn retry(engine: &Engine, runs: &mut [Timed<'_>], waited: &mut Duration) -> Option<f64> {
    let mut probe = None;
    for _ in 0..RETRIES {
        if !runs.iter().any(Timed::over) {
            return None;
        }
        for run in runs.iter_mut().filter(|run| run.over()) {
            let before = match probe {
                Some(ratio) if ratio <= SLOWEST => ratio,
                _ => settled(engine, waited).0,
            };
            if before > SLOWEST {
                return Some(before);
            }
            let (worst, parks, _) = gaps(engine, run.setup, &run.src);
            let after = speed(engine).0;
            probe = Some(after);
            let limit = LIMIT.mul_f64(before.min(after).clamp(0.5, SLOWEST));
            run.tries.push((worst, limit));
            if worst.as_secs_f64() / limit.as_secs_f64()
                < run.worst.as_secs_f64() / run.limit.as_secs_f64()
            {
                (run.worst, run.parks, run.limit) = (worst, parks, limit);
            }
        }
    }
    None
}

/// Prints each row's verdict and fails on a bounded row over its limit,
/// naming its index, setup and program, and every run it took. With
/// `unjudged`, the ratio at which [`retry`] stopped, a row still over is
/// not judged, and the test fails as unable to judge this machine.
fn report(runs: &[Timed<'_>], unjudged: Option<f64>) {
    let mut over = Vec::new();
    for run in runs {
        let tag = match (run.row, run.worst <= run.limit) {
            (Row::Bounded | Row::OpenOnMimalloc, true) => "ok",
            (Row::Bounded | Row::OpenOnMimalloc, false) if unjudged.is_some() => "unjudged",
            (Row::Bounded | Row::OpenOnMimalloc, false) => "OVER",
            (Row::Open, true) => "open (under the limit now: mark it Bounded)",
            (Row::Open, false) => "open",
        };
        let (index, worst, parks, shown) = (run.index, run.worst, run.parks, &run.shown);
        let mut tries = String::new();
        for (step, limit) in &run.tries {
            let _ = write!(tries, " {step:.1?}/{limit:.1?}");
        }
        if !tries.is_empty() {
            tries = format!("  retries (step/limit):{tries}");
        }
        println!("steps: #{index:<3} {worst:>9.1?} {parks:>7} parks  {tag:<5} {shown}{tries}");
        if run.over() {
            over.push(format!(
                "row #{index}: {worst:?} against {:?}{tries}\nsetup:\n{}\nprogram:\n{}",
                run.limit, run.setup, run.src
            ));
        }
    }
    if over.is_empty() {
        return;
    }
    let over = over.join("\n\n");
    if let Some(ratio) = unjudged {
        let refused = format!(
            "this machine took {ratio:.2} times the reference's time after {PATIENCE:?} of \
             probing, so the step oracle cannot judge the rows still over the limit; \
             not judged:\n{over}"
        );
        println!("steps: {refused}");
        panic!("{refused}");
    }
    println!("steps: rows over the limit:\n{over}");
    panic!("native steps over the limit:\n{over}");
}

#[test]
#[cfg_attr(debug_assertions, ignore = "release only: cargo xtask check runs it")]
fn no_native_step_runs_longer_than_the_limit() {
    raise_mmap_threshold();
    let engine = Engine::new(EngineConfig::default());
    // The probes run before and after the rows and the faster counts: a
    // load spike can only slow a probe, which would loosen the limit. The
    // first one fails the test at once on a machine too slow to judge,
    // before the rows' quarter of an hour.
    let mut waited = Duration::ZERO;
    let before = judging(&engine, &mut waited);
    println!(
        "steps: this machine runs at {:.2} of the reference's time before the rows",
        before.0
    );
    let mut runs: Vec<_> = SITES
        .iter()
        .enumerate()
        .map(|(index, &(row, setup, op, times))| {
            let src = format!("{op}\n").repeat(times);
            Timed::new(&engine, (index, row.timed(), setup, src, shown(op, times)))
        })
        .collect();
    let after = speed(&engine);
    let (ratio, fresh, scan, loads) = if after.0 < before.0 { after } else { before };
    // At most `before`, which `judging` holds to `SLOWEST`.
    let limit = LIMIT.mul_f64(ratio.max(0.5));
    println!(
        "steps: limit {limit:.1?}: this machine runs at {ratio:.2} of the reference's time \
         (fresh 64 MiB {fresh:.1?}, scan {scan:.1?}, loads {loads:.1?}; before {:.2}, after {:.2})",
        before.0, after.0
    );
    for run in &mut runs {
        run.limit = limit;
    }
    let unjudged = retry(&engine, &mut runs, &mut waited);
    report(&runs, unjudged);
}

/// The fuel half of [`no_native_step_runs_longer_than_the_limit`]: on the
/// same rows, no slice charges more than [`OVERDRAFT`] past what it was
/// given. Fuel is deterministic, so `cargo xtask check` runs it beside the
/// other gates rather than alone (the `slice_charges` tests).
#[test]
#[cfg_attr(debug_assertions, ignore = "release only: cargo xtask check runs it")]
fn no_slice_charges_more_than_one_step_past_its_fuel() {
    let engine = Engine::new(EngineConfig::default());
    let mut over = Vec::new();
    for (index, &(row, setup, op, times)) in SITES.iter().enumerate() {
        let drawn = overdraft(&engine, setup, &format!("{op}\n").repeat(times));
        let first = shown(op, times);
        println!("steps: #{index:<3} {drawn:>10} past a slice  {first}");
        if row != Row::Open && drawn > OVERDRAFT {
            over.push(format!("row #{index}, {first}: {drawn} units past a slice"));
        }
    }
    assert!(
        over.is_empty(),
        "native steps {OVERDRAFT} units past a slice: {over:#?}"
    );
}

/// A row's operation as its report shows it: its first line, and how many
/// times it repeats.
fn shown(op: &str, times: usize) -> String {
    let first = op.lines().next().unwrap_or_default();
    if times > 1 {
        format!("{first} (x{times})")
    } else {
        first.to_owned()
    }
}

/// Queue loops model-written code uses, at about the sizes it uses them:
/// (setup, loop, wall budget on the reference machine, most fuel or
/// `None`). `set.pop()` starts past the slot it last took (a finger, as
/// `set_pop_impl` keeps): scanning from slot 0 made the loop quadratic,
/// 6.6 s here against 18 ms [M], and its fuel shows it on any machine. A
/// list's tail moves in place, as by `memmove`: copying it through
/// temporaries or a new buffer made `pop(0)` and `insert(0, x)` 13 to 20
/// times slower [M], which only the wall time shows (both charge a unit
/// per 8 items moved).
const QUEUES: &[(&str, &str, Duration, Option<u64>)] = &[
    (
        "s = set(range(200_000))",
        "while s:\n    s.pop()",
        Duration::from_millis(200),
        // 2 units an item; scanning from slot 0, about 2.5e9 [M].
        Some(3_200_000),
    ),
    (
        "q = list(range(50_000))",
        "while q:\n    q.pop(0)",
        Duration::from_millis(1_000),
        None,
    ),
    (
        "q = []",
        "for i in range(50_000):\n    q.insert(0, i)",
        Duration::from_millis(1_000),
        None,
    ),
];

#[test]
#[cfg_attr(debug_assertions, ignore = "release only: cargo xtask check runs it")]
fn queue_loops_run_at_cpython_speed() {
    let engine = Engine::new(EngineConfig::default());
    let ratio = judging(&engine, &mut Duration::default()).0.max(0.5);
    let mut slow = Vec::new();
    for &(setup, src, budget, most) in QUEUES {
        let code = engine.compile(src, "queue.py").expect("compiles");
        let budget = budget.mul_f64(ratio);
        // The least of three runs, for a loaded machine.
        let mut best = Duration::MAX;
        let mut fuel = 0;
        for _ in 0..3 {
            let mut s = Session::new(&engine).expect("session");
            let p = s.start(&engine.compile(setup, "setup.py").expect("setup compiles"));
            assert!(matches!(p, Progress::Complete(_)), "setup: {p:?}");
            s.set_fuel_total(Some(u64::MAX / 2));
            let t = Instant::now();
            let p = s.start(&code);
            best = best.min(t.elapsed());
            assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
            fuel = u64::MAX / 2 - s.fuel_total().expect("a total");
            if best <= budget {
                break;
            }
        }
        let first = src.lines().last().unwrap_or_default().trim();
        println!("queues: {best:>9.1?} (budget {budget:.1?}) {fuel:>12} units  {first}");
        if best > budget {
            slow.push(format!("{first}: {best:?}, over {budget:?}"));
        }
        if let Some(most) = most
            && fuel > most
        {
            slow.push(format!("{first}: {fuel} units, over {most}"));
        }
    }
    assert!(slow.is_empty(), "queue loops too slow: {slow:#?}");
}

/// The opt-in modules (P5): `unicodedata` normalizes a long str a slice a
/// step (`nstep::op::UNI_NORM`); `ast.parse` parses and builds its tree in
/// one step, its source capped at 32 KiB (decision (c)).
#[cfg(any(feature = "mod-unicodedata", feature = "mod-ast"))]
const OPT_IN: &[(Row, &str, &str)] = &[
    #[cfg(feature = "mod-unicodedata")]
    (
        Row::Bounded,
        "import unicodedata\ns = '\\xe9' * 5_000_000",
        "t = unicodedata.normalize('NFD', s)",
    ),
    #[cfg(feature = "mod-unicodedata")]
    (
        Row::Bounded,
        "import unicodedata\ns = 'e\\u0301' * 3_000_000",
        "t = unicodedata.normalize('NFC', s)",
    ),
    #[cfg(feature = "mod-unicodedata")]
    (
        Row::Bounded,
        "import unicodedata\ns = '\\ufdfa' * 500_000",
        "t = unicodedata.normalize('NFKD', s)",
    ),
    #[cfg(feature = "mod-unicodedata")]
    (
        Row::Bounded,
        "import unicodedata\ns = 'a' * 60_000_000",
        "b = unicodedata.is_normalized('NFKC', s)",
    ),
    // A run with no slice boundary is normalized in one step, so it is
    // capped at 65,536 chars after decomposition (P5): one char under it
    // (one mark decomposing to two, the slowest shape: 15 ms [M]), and
    // 1M combining marks, a MemoryError once the scan reaches the cap
    // (161 ms in one step before the cap [M]).
    #[cfg(feature = "mod-unicodedata")]
    (
        Row::Bounded,
        "import unicodedata\ns = 'x' + '\\u0344' * 32_767 + '\\u0301'",
        "t = unicodedata.normalize('NFKC', s)",
    ),
    #[cfg(feature = "mod-unicodedata")]
    (
        Row::Bounded,
        "import unicodedata\ns = 'x' + '\\u0301' * 1_000_000",
        "try:\n    unicodedata.normalize('NFC', s)\nexcept MemoryError:\n    pass",
    ),
    // One small statement every two bytes, the worst shape found [M].
    #[cfg(feature = "mod-ast")]
    (
        Row::Bounded,
        "import ast\nsrc = 'a\\n' * 16_384",
        "t = ast.parse(src)",
    ),
    #[cfg(feature = "mod-ast")]
    (
        Row::Bounded,
        "import ast\nsrc = '\\xe9 = 1\\n' * 4_681",
        "t = ast.parse(src)",
    ),
];

#[test]
#[cfg(any(feature = "mod-unicodedata", feature = "mod-ast"))]
#[cfg_attr(debug_assertions, ignore = "release only: cargo xtask check runs it")]
fn no_opt_in_module_step_runs_longer_than_the_limit() {
    raise_mmap_threshold();
    let engine = Engine::new(EngineConfig::default());
    let mut waited = Duration::ZERO;
    let limit = LIMIT.mul_f64(judging(&engine, &mut waited).0.max(0.5));
    let mut runs: Vec<_> = OPT_IN
        .iter()
        .enumerate()
        .map(|(index, &(row, setup, op))| {
            Timed::new(&engine, (index, row, setup, op.to_owned(), shown(op, 1)))
        })
        .collect();
    for run in &mut runs {
        run.limit = limit;
    }
    let unjudged = retry(&engine, &mut runs, &mut waited);
    report(&runs, unjudged);
}

/// [`no_slice_charges_more_than_one_step_past_its_fuel`] on the opt-in
/// modules' rows.
#[test]
#[cfg(any(feature = "mod-unicodedata", feature = "mod-ast"))]
#[cfg_attr(debug_assertions, ignore = "release only: cargo xtask check runs it")]
fn no_opt_in_module_slice_charges_more_than_one_step_past_its_fuel() {
    let engine = Engine::new(EngineConfig::default());
    let mut over = Vec::new();
    for (index, &(row, setup, op)) in OPT_IN.iter().enumerate() {
        let drawn = overdraft(&engine, setup, op);
        println!("steps: #{index:<3} {drawn:>10} past a slice  {op}");
        if row == Row::Bounded && drawn > OVERDRAFT {
            over.push(format!("row #{index}, {op}: {drawn} units past a slice"));
        }
    }
    assert!(
        over.is_empty(),
        "native steps {OVERDRAFT} units past a slice: {over:#?}"
    );
}

/// A restore leaves a bundled program's instructions out until one of its
/// codes first runs (docs/spec.md §9.3), so the step that enters it also
/// unpacks and decodes them, on a fresh engine with its decoder tables to
/// build: no such step runs longer than the limit. The rows are the
/// largest bundled modules (`decimal` packs to 28 KB, `datetime` to
/// 19 KB) and the most imported one; each also times its import on a fresh
/// engine, which decodes the whole module in one step.
#[test]
#[cfg_attr(debug_assertions, ignore = "release only: cargo xtask check runs it")]
fn no_first_run_of_restored_bundled_code_runs_longer_than_the_limit() {
    let engine = Engine::new(EngineConfig::default());
    let limit = LIMIT.mul_f64(judging(&engine, &mut Duration::default()).0.max(0.5));
    // The longest step of `src` in `s`, parking at every safepoint that
    // reads the clock.
    let worst_step = |s: &mut Session, src: &str| {
        let code = s.compile(src, "step.py").expect("compiles");
        let mut worst = Duration::ZERO;
        let mut t = Instant::now();
        s.set_deadline(Some(t));
        let mut p = s.start(&code);
        loop {
            worst = worst.max(t.elapsed());
            if p != (Progress::Parked {
                reason: ParkReason::Deadline,
            }) {
                break;
            }
            t = Instant::now();
            s.set_deadline(Some(t));
            p = s.resume(Resume::Continue).expect("resumes");
        }
        assert!(matches!(p, Progress::Complete(_)), "{src}: {p:?}");
        s.set_deadline(None);
        worst
    };
    for (setup, src) in [
        ("import decimal", "x = decimal.Decimal('1.25') * 3"),
        (
            "import datetime",
            "x = datetime.date(2026, 11, 19).isoformat()",
        ),
        (
            "import collections",
            "x = collections.Counter('abracadabra')",
        ),
    ] {
        let engine = Engine::new(EngineConfig::default());
        let mut s = Session::new(&engine).expect("session");
        let import = worst_step(&mut s, setup);
        let image = s.snapshot().expect("snapshot");
        let fresh = Engine::new(EngineConfig::default());
        let mut s = fresh.restore(&image).expect("restore");
        let worst = worst_step(&mut s, src);
        println!("steps: {setup}: longest step {import:.2?}; restored, then {src}: {worst:.2?}");
        assert!(import < limit, "{setup}: {import:?} against {limit:?}");
        assert!(
            worst < limit,
            "{setup}, restored: {worst:?} against {limit:?}"
        );
    }
}
