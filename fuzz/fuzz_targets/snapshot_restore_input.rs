//! The `snapshot_restore` fuzz target's input decoding and driver, shared with
//! the regression test that replays its seeds
//! (`crates/micro-monty/tests/snapshots.rs`), so a seed means the same
//! edit to the same image in both.

use micro_monty::{
    Engine, EngineConfig, HostClass, HostErrorKind, HostException, HostModule, HostParam,
    HostSignature, HostValue, Limits, ParamKind, Progress, Resume, Session, SnapshotError,
};
use std::sync::OnceLock;

const PROGRAMS: [&str; 7] = [
    // Idle, with classes, closures and host functions in the globals.
    "class P:\n    def __init__(self, x):\n        self.x = x\n    def __repr__(self):\n        return f'P({self.x})'\n\
     def mk(n):\n    return lambda: n\nps = [P(i) for i in range(30)]\nfs = [mk(i) for i in range(5)]\n\
     d = {str(i): (i, b'x' * i, 2 ** (i * 5)) for i in range(20)}\ns = {1, 2, 'three'}\n\
     import files\nes = [files.Entry(str(i), i) for i in range(3)]\nE = files.Gone\n",
    // Parked inside a generator inside a method.
    "class W:\n    def __init__(self, n):\n        self.n = n\n    def items(self):\n        def gen(k):\n\
     \x20           for i in range(k):\n                try:\n                    yield {'i': i, 's': str(i)}\n\
     \x20               finally:\n                    pass\n        t = 0\n        for d in gen(self.n):\n\
     \x20           t += d['i']\n        return t\nprint(W(10000).items())\n",
    // A host call from inside a sort key.
    "def key(x):\n    return ask(x)\nprint(sorted([3, 1, 2], key=key))\n",
    // Waiting on host futures from tasks.
    "import asyncio\nasync def one(i):\n    await asyncio.sleep(0)\n    return await slow(i)\n\
     async def main():\n    return await asyncio.gather(*[one(i) for i in range(3)])\nprint(asyncio.run(main()))\n",
    // Parked in recursion through dunders and a with block.
    "class C:\n    def __init__(self, n):\n        self.n = n\n    def __add__(self, o):\n\
     \x20       return C(self.n + o.n) if self.n < 3000 else self\n    def __enter__(self):\n        return self\n\
     \x20   def __exit__(self, *a):\n        return False\nwith C(1) as c:\n    x = c\n    while x.n < 3000:\n\
     \x20       x = x + C(1)\nprint(x.n)\n",
    // Multiple bundled programs share one bounded decoder workspace on restore.
    "import collections\nimport functools\ncounts = collections.Counter('abaca')\n",
    // Parked in a user iterator under star-unpacking.
    "class It:\n    def __init__(self):\n        self.i = 0\n    def __iter__(self):\n        return self\n\
     \x20   def __next__(self):\n        self.i += 1\n        if self.i > 3000:\n            raise StopIteration\n\
     \x20       return self.i\na, *b = It()\nprint(a, len(b))\n",
];

fn config() -> EngineConfig {
    EngineConfig::new().with_limits(Limits::new().with_memory(64 << 20))
}

/// The engine that takes the base images and restores most inputs: its
/// module cache holds the host modules' compiled code, which the images
/// refer to by key (`CODE_HOST`).
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine::new(config()))
}

fn param(name: &str, annotation: &str, default: Option<HostValue>) -> HostParam {
    HostParam {
        name: name.into(),
        annotation: annotation.into(),
        default,
        kind: ParamKind::PositionalOrKeyword,
    }
}

fn session(e: &Engine) -> Session {
    let mut s = Session::new(e).expect("session");
    s.define(&HostModule::untyped("", &["ask", "slow"]))
        .expect("host functions");
    // Typed modules have generated code the engine shares; `tools.get`
    // names `files.Entry`, so its key has a recipe.
    s.define(&HostModule {
        name: "files".into(),
        functions: vec![HostSignature {
            name: "read".into(),
            params: vec![
                param("path", "str", None),
                param("n", "int | None", Some(HostValue::None)),
            ],
            returns: "str".into(),
            doc: "Read a file.".into(),
            dict_as_kwargs: false,
        }],
        classes: vec![HostClass {
            name: "Entry".into(),
            fields: vec![("path".into(), "str".into()), ("size".into(), "int".into())],
            methods: vec![HostSignature {
                name: "open".into(),
                params: vec![param("limit", "int", Some(HostValue::Int(10)))],
                returns: "Entry".into(),
                ..HostSignature::default()
            }],
            doc: "An entry.".into(),
        }],
        constants: Vec::new(),
        exceptions: vec![HostException {
            name: "Gone".into(),
            base: HostErrorKind::FileNotFoundError,
            doc: String::new(),
        }],
    })
    .expect("files");
    s.define(&HostModule {
        constants: vec![
            ("K".into(), HostValue::Int(7)),
            (
                "rows".into(),
                HostValue::List(vec![HostValue::Dict(vec![(
                    "record".into(),
                    HostValue::Record {
                        name: "Row".into(),
                        fields: vec![
                            ("bytes".into(), HostValue::Bytes(vec![17; 256])),
                            ("empty".into(), HostValue::List(Vec::new())),
                        ],
                    },
                )])]),
            ),
        ],
        functions: vec![HostSignature {
            name: "get".into(),
            params: vec![param("e", "files.Entry", None)],
            returns: "files.Entry | None".into(),
            ..HostSignature::default()
        }],
        ..HostModule::untyped("tools", &[])
    })
    .expect("host module");
    s.set_output(Box::new(|_| {}));
    s
}

/// Valid snapshots of the programs, taken at their first stop (after some
/// fuel for the parked ones): `[0]` refers to the engine's host code by
/// key, `[1]` stores it (`Engine::with_host_code_in_snapshots`).
fn bases() -> &'static [Vec<Vec<u8>>; 2] {
    static BASES: OnceLock<[Vec<Vec<u8>>; 2]> = OnceLock::new();
    BASES.get_or_init(|| {
        [
            images(engine()),
            images(&engine().with_host_code_in_snapshots(true)),
        ]
    })
}

fn images(e: &Engine) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for (k, src) in PROGRAMS.iter().enumerate() {
        let mut s = session(e);
        let code = e.compile(src, "fuzz.py").expect("compiles");
        s.set_fuel(Some(if matches!(k, 0 | 5) { u64::MAX } else { 1_500 }));
        let mut p = s.start(&code);
        if k == 3 {
            // Answer the host calls with futures until the tasks wait.
            while let Progress::HostCall { .. } = p {
                p = s.resume(Resume::Future).expect("future");
            }
        }
        if matches!(k, 0 | 5) {
            assert!(matches!(p, Progress::Complete(_)), "program {k}: {p:?}");
        }
        out.push(s.snapshot().expect("snapshot"));
    }
    out
}

/// Where the header's stop starts: after the magic, the format, the build
/// id and the two last-expression flags (`micro-monty/src/image.rs`).
const STOP_AT: usize = 44;

/// The header's stop phase: 0 idle, 1 a host call, 2 waiting, 3 parked.
#[must_use]
pub fn stop_phase(image: &[u8]) -> Option<u8> {
    image.get(STOP_AT).copied()
}

/// `image` with its header's stop rewritten to one of `phase` (as
/// [`stop_phase`] numbers them), the rest unchanged and the checksum
/// stale: a host call to `ask` with no arguments, a wait on one call, a
/// fuel park. Its call id is one the image names (a waiting image's first
/// call, for a host call; a host call's, for a wait), else 1. An image
/// already at `phase` keeps its own stop. `None` if the stop does not
/// parse.
#[must_use]
pub fn forge_stop(image: &[u8], phase: u8) -> Option<Vec<u8>> {
    let body = image.get(STOP_AT..)?;
    let mut r = mm_core::Reader::new(body);
    let mut budget = u64::MAX;
    let mut call = 1;
    match r.u8()? {
        0 => {}
        1 => {
            call = r.u64()?;
            r.u32()?;
            r.str()?;
            for _ in 0..r.count(1)? {
                mm_engine::read_host_value(&mut r, &mut budget)?;
            }
            for _ in 0..r.count(5)? {
                r.str()?;
                mm_engine::read_host_value(&mut r, &mut budget)?;
            }
        }
        2 => {
            let n = r.count(8)?;
            call = r.u64()?;
            for _ in 1..n {
                r.u64()?;
            }
        }
        3 => {
            r.u8()?;
        }
        _ => return None,
    }
    if stop_phase(image) == Some(phase) {
        return Some(image.to_vec());
    }
    let end = STOP_AT + body.len() - r.remaining();
    let mut stop = vec![phase];
    match phase {
        1 => {
            stop.extend_from_slice(&call.to_le_bytes());
            stop.extend_from_slice(&0u32.to_le_bytes());
            stop.extend_from_slice(&3u32.to_le_bytes());
            stop.extend_from_slice(b"ask");
            stop.extend_from_slice(&[0; 8]);
        }
        2 => {
            stop.extend_from_slice(&1u32.to_le_bytes());
            stop.extend_from_slice(&call.to_le_bytes());
        }
        3 => stop.push(0),
        _ => {}
    }
    let mut out = image[..STOP_AT].to_vec();
    out.extend_from_slice(&stop);
    out.extend_from_slice(&image[end..]);
    Some(out)
}

/// Recomputes the checksum of image `b`, so an edit reaches the decoder.
pub fn reseal(b: &mut [u8]) {
    if b.len() < 8 {
        return;
    }
    let n = b.len() - 8;
    let sum = mm_core::xxh64(&b[..n], 0);
    b[n..].copy_from_slice(&sum.to_le_bytes());
}

/// The most fuel a parked operation may ask for and still be resumed.
const MAX_WANTED: u64 = 1 << 20;

fn drive(e: &Engine, mut s: Session) {
    s.set_output(Box::new(|_| {}));
    s.set_fuel(Some(1_000));
    // An int operation the fuel could not pay parks before it starts, and
    // the next resume runs it whatever the fuel (docs/spec.md §5, "Natives
    // charge fuel"): a host with this budget would not resume it, so the
    // target does not either (seconds per input otherwise).
    let mut p = if s.fuel_wanted() > MAX_WANTED {
        Progress::Complete(None)
    } else {
        match s.resume(Resume::Continue) {
            Ok(p) => p,
            Err(_) => match s.resume(Resume::Value(HostValue::Int(1))) {
                Ok(p) => p,
                Err(_) => s
                    .resume(Resume::Batch(Vec::new()))
                    .unwrap_or(Progress::Complete(None)),
            },
        }
    };
    for _ in 0..4 {
        s.set_fuel(Some(1_000));
        let r = match p {
            Progress::Parked { .. } if s.fuel_wanted() > MAX_WANTED => break,
            Progress::Parked { .. } => Resume::Continue,
            Progress::HostCall { .. } => Resume::Value(HostValue::None),
            Progress::Waiting { ref calls } => Resume::Batch(
                calls
                    .iter()
                    .map(|&c| (c, Ok(HostValue::Str("r".into()))))
                    .collect(),
            ),
            _ => break,
        };
        p = match s.resume(r) {
            Ok(p) => p,
            Err(_) => break,
        };
    }
    let _ = s.snapshot();
    s.cancel();
    s.set_fuel(Some(1_000));
    if let Ok(code) = e.compile("x = [str(i) for i in range(50)]\nlen(x)", "after.py") {
        let _ = s.start(&code);
    }
}

/// One fuzz input: raw bytes, or edits to a valid snapshot (see the target's
/// doc comment); restores and drives the session.
pub fn run(data: &[u8]) {
    let Some((&mode, rest)) = data.split_first() else {
        return;
    };
    // Bit 3: restore on an engine without a module cache, which must build
    // the host code again from the image's definitions and check its key.
    // Bit 4: edit the image that stores its host code. Bit 5: see below.
    let uncached = Engine::with_module_cache(config(), 0);
    let e = if mode & 0x08 != 0 {
        &uncached
    } else {
        engine()
    };
    let bytes = if mode % 8 == 0 {
        rest.to_vec()
    } else {
        let Some((&pick, edits)) = rest.split_first() else {
            return;
        };
        // Pick 0xFE is the last program, added after seeds were stored:
        // every other pick keeps the image it had. Picks 0xE0..=0xEF (no
        // seed stored before them used one) take program `pick >> 2 & 3`
        // (idle, parked, at a host call, waiting) with its header's stop
        // forged to phase `pick & 3` (`forge_stop`) before the edits.
        let forged = (0xE0..=0xEF).contains(&pick).then_some(pick & 3);
        let k = if pick == 0xFE {
            PROGRAMS.len() - 1
        } else if forged.is_some() {
            usize::from(pick >> 2 & 3)
        } else {
            pick as usize % (PROGRAMS.len() - 1)
        };
        let base = &bases()[usize::from(mode & 0x10 != 0)][k];
        let mut b = match forged.and_then(|phase| forge_stop(base, phase)) {
            Some(f) => f,
            None => base.clone(),
        };
        // Edits of 4 bytes: a 16-bit position (from the start, or within
        // the last 8 KiB for the VM-state tail), a xor byte, and a byte
        // that may truncate (0xFF), delete the byte (0xFE) or insert the
        // xor byte before it (0xFD) instead, shifting what follows.
        let tail = mode % 8 >= 4;
        for ed in edits.as_chunks::<4>().0 {
            let pos = usize::from(u16::from_le_bytes([ed[0], ed[1]]));
            let len = b.len();
            if len <= 8 {
                break;
            }
            let at = if tail {
                len.saturating_sub(8 + 8192) + pos % 8192.min(len - 8)
            } else {
                (pos * 37 + usize::from(ed[3]) * 65_536 * 37) % (len - 8)
            };
            if ed[3] == 0xFF {
                b.truncate(at + 8);
            } else if ed[3] == 0xFE {
                b.remove(at);
            } else if ed[3] == 0xFD {
                b.insert(at, ed[2]);
            } else if let Some(x) = b.get_mut(at) {
                *x ^= ed[2];
            }
        }
        if mode % 4 != 3 {
            reseal(&mut b);
        }
        b
    };
    let restored = e.restore(&bytes);
    // Bit 5: restore on the other engine too. The cached one takes a
    // module's code by key when the image's definition is the one it was
    // built from, the uncached one builds it from the definition; either
    // way an image whose definition does not build its key is refused.
    if mode & 0x20 != 0 {
        let host_code = |r: &Result<Session, SnapshotError>| matches!(r, Err(SnapshotError::Corrupt { what }) if what == "host code");
        if restored.is_ok() || host_code(&restored) {
            let other = if mode & 0x08 != 0 {
                engine()
            } else {
                &uncached
            };
            let again = other.restore(&bytes);
            assert!(
                !(host_code(&restored) ^ host_code(&again)),
                "host code verdicts differ: {:?} and {:?}",
                restored.as_ref().err(),
                again.as_ref().err()
            );
        }
    }
    if let Ok(s) = restored {
        drive(e, s);
    }
}
