//! Snapshot images written by an earlier build, kept in `tests/golden/`
//! (docs/spec.md §9.3, "Compatibility"). Under the snapshot build id that
//! wrote them, each image restores and continues exactly as the session
//! that wrote it did, however the code changed since: the id covers what
//! an image's meaning rests on, not the source. Under another id (after a
//! layout, table, bytecode or codec change) an image is refused before
//! anything else is read, as `Incompatible`.
//!
//! A change that moves the id blesses the images again in the same commit:
//!
//! ```text
//! MM_BLESS=1 cargo test --release --locked -p micro-monty --test golden
//! ```
//!
//! It runs each scenario afresh and writes its image again, but writes
//! nothing and fails if a session's continuation differs from the
//! committed transcript (`*.txt`), so blessing never hides a change in
//! behavior. To change a transcript on purpose, delete it and bless again;
//! the commit then shows the new one. `cargo xtask check` runs this test
//! the same way and fails while the images carry another id. Debug builds
//! skip it: under `cargo test --workspace`, mm-cli's opt-in modules unify
//! into the facade and give it another id.
#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    Engine, EngineConfig, HostModule, HostParam, HostSignature, HostValue, Progress, Resume,
    Session, SnapshotError,
};
use std::{
    fmt::Write as _,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// A cell that parks at its first host call, inside a keyed sort, over a
/// heap of most builtin types, user classes, closures, a suspended
/// generator and live iterators.
const SYNC: &str = r#"
import json
from collections import OrderedDict, defaultdict, Counter, deque, namedtuple

class Shape:
    count = 0
    def __init__(self, name, *dims):
        self.name = name
        self.dims = dims
        Shape.count += 1
    def area(self):
        r = 1
        for d in self.dims:
            r *= d
        return r
    def __repr__(self):
        return f"{type(self).__name__}({self.name!r}, {self.dims})"

class Square(Shape):
    def __init__(self, side):
        super().__init__("square", side, side)
    @property
    def side(self):
        return self.dims[0]
    @staticmethod
    def unit():
        return Square(1)
    @classmethod
    def named(cls):
        return cls.__name__

Point = namedtuple("Point", "x y")

def squares(n):
    for i in range(n):
        yield i * i

def counter_from(start):
    count = start
    def bump(by=1):
        nonlocal count
        count += by
        return count
    return bump

gen = squares(10)
first = [next(gen), next(gen)]
bump = counter_from(100)
bump()
bump(5)
it = iter([10, 20, 30])
next(it)
en = enumerate("xyz")
next(en)
zp = zip("ab", [1, 2])
rv = reversed([1, 2, 3])
next(rv)
data = {
    "ints": [1, -2, 3 ** 80, 2 ** 64],
    "floats": (0.5, -1e300, float("inf")),
    "nested": {"a": [1, {"b": (2, 3)}], (1, 2): frozenset({4, 5})},
    "bytes": b"\x00\xffmm",
    "ba": bytearray(b"hello"),
    "c": complex(1.5, -2),
    "r": range(3, 30, 4),
    "s": slice(1, None, 2),
    "set": {1, "two", (3,)},
}
od = OrderedDict(a=1, b=2)
dd = defaultdict(list)
dd["k"].append(1)
ctr = Counter("abracadabra")
dq = deque([1, 2, 3], maxlen=5)
shapes = [Shape("box", 2, 3, 4), Square(5), Square.unit()]
pt = Point(3, 4)
try:
    {}["missing"]
except KeyError as e:
    caught = e
view = data.keys()
text = json.dumps({"k": [1, 2, {"x": None}]}, sort_keys=True)
words = ["pear", "fig", "banana", "kiwi"]
ranked = sorted(words, key=lookup)
rest = [next(gen), next(it), next(en), list(zp), next(rv)]
print(first, rest, bump(), ranked)
print(sorted(data), data["ints"][2] % 1000, data["c"], data["r"][2], data["s"])
print(od, dict(dd), ctr.most_common(2), dq, pt, pt._replace(x=9))
print([repr(s) for s in shapes], [s.area() for s in shapes], shapes[1].side, Square.named(), Shape.count)
print(repr(caught), sorted(view, key=str), text, data["ba"].upper(), data["bytes"].hex())
f"{len(ranked)}:{ranked[0]}"
"#;

/// A cell whose tasks all wait on host futures under `asyncio.gather`.
const ASYNC: &str = r#"
import asyncio

async def get(k):
    v = await fetch(k)
    return k, v * 2

async def main():
    pairs = await asyncio.gather(get("a"), get("bb"), get("ccc"))
    return dict(pairs)

out = await main()
print(sorted(out.items()))
out
"#;

const SCENARIOS: [(&str, &str); 2] = [("sync", SYNC), ("async", ASYNC)];

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn engine() -> Engine {
    Engine::new(EngineConfig::default())
}

/// `lookup(key: str) -> int` (answered with the key's length) and the
/// async `fetch(key: str) -> int` (answered by call id), in the builtins.
fn host() -> HostModule {
    let typed = |name: &str| HostSignature {
        name: name.to_owned(),
        params: vec![HostParam {
            name: "key".to_owned(),
            annotation: "str".to_owned(),
            ..HostParam::default()
        }],
        returns: "int".to_owned(),
        ..HostSignature::default()
    };
    HostModule {
        name: String::new(),
        functions: vec![typed("lookup"), typed("fetch")],
        ..HostModule::default()
    }
}

/// Collects the session's output.
fn capture(s: &mut Session) -> Arc<Mutex<String>> {
    let out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&out);
    s.set_output(Box::new(move |t| {
        if let Ok(mut o) = sink.lock() {
            o.push_str(t);
        }
    }));
    out
}

/// Runs `s` from stop `p` to the end of its cell, answering every host
/// call by the script, and returns the transcript: each stop, the output
/// and how the cell ended.
fn finish(s: &mut Session, mut p: Progress, out: &Mutex<String>) -> String {
    let mut t = String::new();
    loop {
        let resume = match p {
            Progress::HostCall {
                call, name, args, ..
            } => {
                let _ = writeln!(t, "call {call} {name} {args:?}");
                match (name.as_str(), args.first()) {
                    ("lookup", Some(HostValue::Str(k))) => {
                        Resume::Value(HostValue::Int(i64::try_from(k.len()).unwrap_or(0)))
                    }
                    _ => Resume::Future,
                }
            }
            Progress::Waiting { calls } => {
                let _ = writeln!(t, "waiting {calls:?}");
                Resume::Batch(
                    calls
                        .iter()
                        .map(|&c| (c, Ok(HostValue::Int(i64::try_from(c).unwrap_or(0) * 10))))
                        .collect(),
                )
            }
            Progress::Parked { .. } => Resume::Continue,
            Progress::Complete(c) => {
                let out = out.lock().map(|o| o.clone()).unwrap_or_default();
                let _ = writeln!(t, "output:\n{out}complete {:?}", c.map(|c| c.repr));
                return t;
            }
            Progress::Error(e) => {
                let _ = writeln!(t, "error {}", e.traceback);
                return t;
            }
            other => panic!("unexpected {other:?}"),
        };
        p = s.resume(resume).expect("resume");
    }
}

/// Writes each scenario's image at its first stop for the host, once the
/// session that wrote it has continued from there to the committed
/// transcript in every scenario (a missing transcript is written).
fn bless() {
    fs::create_dir_all(dir()).expect("tests/golden");
    let mut blessed = Vec::new();
    let mut changed = String::new();
    for (name, cell) in SCENARIOS {
        let e = engine();
        let mut s = Session::new(&e).expect("session");
        s.define(&host()).expect("define");
        let out = capture(&mut s);
        let mut p = s.start(&e.compile(cell, &format!("{name}.py")).expect("compile"));
        while let Progress::HostCall { ref name, .. } = p
            && name == "fetch"
        {
            p = s.resume(Resume::Future).expect("resume");
        }
        assert!(
            matches!(p, Progress::HostCall { .. } | Progress::Waiting { .. }),
            "{name}: {p:?}"
        );
        let image = s.snapshot().expect("snapshot");
        if let Ok(mut o) = out.lock() {
            o.clear();
        }
        let transcript = finish(&mut s, p, &out);
        let txt = dir().join(format!("{name}.txt"));
        if txt.exists() {
            let committed = fs::read_to_string(&txt).expect("transcript");
            if committed != transcript {
                let _ = write!(
                    changed,
                    "\n{name}.txt, committed:\n{committed}{name}, now:\n{transcript}"
                );
            }
        } else {
            fs::write(&txt, &transcript).expect("write transcript");
        }
        blessed.push((name, image));
    }
    assert!(
        changed.is_empty(),
        "nothing blessed: a continuation differs from its committed transcript. \
         If the change is intended, delete the transcript, bless again and say \
         why in the commit.{changed}"
    );
    for (name, image) in blessed {
        fs::write(dir().join(format!("{name}.mmsnap")), image).expect("write image");
    }
}

fn golden(name: &str) -> (Vec<u8>, [u8; 32]) {
    let image = fs::read(dir().join(format!("{name}.mmsnap"))).expect("a blessed image");
    let id = micro_monty::image_build_id(&image).expect("an image");
    (image, id)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release only, in the facade's own build: \
              cargo test --release --locked -p micro-monty --test golden"
)]
fn golden_images_continue_under_the_build_id_that_wrote_them() {
    if std::env::var_os("MM_BLESS").is_some() {
        bless();
    }
    for (name, _) in SCENARIOS {
        let (image, id) = golden(name);
        assert!(
            &id == micro_monty::build_id(),
            "{name}: the golden images carry snapshot build id {:02x?}…, this \
             build has {:02x?}…. A change to a hashed codec, layout, table or \
             bytecode, or to SNAPSHOT_EPOCH, moved it (docs/spec.md §9.3); bless \
             them again in the same commit: \
             MM_BLESS=1 cargo test --release --locked -p micro-monty --test golden",
            &id[..4],
            &micro_monty::build_id()[..4]
        );
        let expected = fs::read_to_string(dir().join(format!("{name}.txt"))).expect("txt");
        let mut s = engine()
            .restore(&image)
            .expect("an image of this build id restores");
        let out = capture(&mut s);
        let p = s.pending().expect("stopped for the host");
        assert_eq!(finish(&mut s, p, &out), expected, "{name}");
    }
}

/// An image whose id another build wrote (as after a layout change) is
/// refused as `Incompatible` before anything else is read.
#[test]
fn a_golden_image_under_another_build_id_is_incompatible() {
    if std::env::var_os("MM_BLESS").is_some() {
        return; // the other test is writing the images
    }
    let (mut image, _) = golden("sync");
    image[10] ^= 0x5a;
    let n = image.len();
    let sum = mm_core::xxh64(&image[..n - 8], 0);
    image[n - 8..].copy_from_slice(&sum.to_le_bytes());
    let id = micro_monty::image_build_id(&image).expect("an image");
    assert_ne!(&id, micro_monty::build_id());
    assert_eq!(
        engine().restore(&image).err(),
        Some(SnapshotError::Incompatible {
            format: micro_monty::SNAPSHOT_FORMAT,
            build_id: id
        })
    );
}
