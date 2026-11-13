//! The snapshot upgrade policy (U1, docs/user-guide.md "Upgrading: drain old
//! builds"): one `Runtime` keeps the old build's guest loaded while new
//! sessions run on the new build, and each image restores on the module of
//! the build that wrote it. An image of a build that is not loaded is
//! refused with `Error::UnknownBuild`.
//!
//! Needs two guests whose build ids differ: `MM_WASM_GUEST` (the new build,
//! default features) and `MM_WASM_GUEST_OLD` (the old build; `cargo xtask
//! wasm-host` builds the guest with the opt-in `mod-uuid` module, which is
//! part of the build id).
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use micro_monty::{
    Engine, EngineConfig, HostModule, HostValue, Limits, ParkReason, Progress, Resume,
    SnapshotError,
};
use mm_wasm_host::{Error, Runtime, RuntimeConfig, Session, SessionOptions, image_build_id};

fn path(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(default),
        PathBuf::from,
    )
}

fn new_guest() -> PathBuf {
    path(
        "MM_WASM_GUEST",
        "../../target/wasm32-unknown-unknown/release/mm_wasm_guest.wasm",
    )
}

fn old_guest() -> PathBuf {
    path(
        "MM_WASM_GUEST_OLD",
        "../../target/mm-wasm-alt/wasm32-unknown-unknown/release/mm_wasm_guest.wasm",
    )
}

/// Uses the old build's `uuid`, two host calls and a loop long enough to
/// park on fuel several times.
const OLD_CELL: &str = "\
import uuid
u = uuid.UUID(int=0x1234)
x = lookup('a')
print('first', x)
total = 0
for i in range(20000):
    total += i * i
y = lookup('bc')
print(u, x, y, total)
total
";

const SLICE: u64 = 3_000;

const OPTIONS: SessionOptions = {
    let mut o = SessionOptions::new().with_limits(
        Limits::new()
            .with_memory(64 << 20)
            .with_recursion_depth(200)
            .with_fuel(Some(SLICE)),
    );
    o.host_args = micro_monty::HostArgs {
        max_bytes: 1 << 20,
        max_depth: 100,
        lenient: false,
    };
    o
};

fn output(s: &mut Session) -> Arc<Mutex<String>> {
    let out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&out);
    s.set_output(Box::new(move |_, t| sink.lock().unwrap().push_str(t)));
    out
}

/// Records `p` and every later stop, answering host calls and granting fuel
/// slices, until the cell ends or `stop_after` stops are recorded. Returns
/// the last stop.
fn drive(
    s: &mut Session,
    mut p: Progress,
    steps: &mut Vec<String>,
    stop_after: Option<usize>,
) -> Progress {
    loop {
        steps.push(format!("{p:?}"));
        if stop_after == Some(steps.len()) {
            return p;
        }
        let resume = match &p {
            Progress::HostCall { args, .. } => match args.first() {
                Some(HostValue::Str(a)) => Resume::Value(HostValue::Str(a.to_uppercase())),
                _ => Resume::Value(HostValue::None),
            },
            Progress::Parked {
                reason: ParkReason::Fuel,
            } => {
                s.set_fuel(Some(SLICE)).unwrap();
                Resume::Continue
            }
            _ => return p,
        };
        p = s.resume(&resume).unwrap();
    }
}

fn started(rt: &Runtime) -> (Session, Arc<Mutex<String>>, Progress) {
    let mut s = rt.session(&OPTIONS).unwrap();
    let out = output(&mut s);
    s.define(&HostModule::untyped("", &["lookup"])).unwrap();
    let p = s.start(OLD_CELL, "cell.py").unwrap();
    (s, out, p)
}

#[test]
fn old_snapshots_drain_on_the_old_build_beside_the_new_one() {
    let mut rt = Runtime::new(&old_guest(), RuntimeConfig::default()).unwrap();
    let old = rt.current_build();

    // The old build, run through without parking: the reference.
    let (mut s, out, p) = started(&rt);
    let mut reference = Vec::new();
    let end = drive(&mut s, p, &mut reference, None);
    assert!(matches!(end, Progress::Complete(_)), "{end:?}");
    let reference_out = out.lock().unwrap().clone();
    assert!(
        reference_out.contains("00000000-0000-0000-0000-000000001234 A BC"),
        "{reference_out}"
    );
    assert!(
        reference.iter().filter(|r| r.contains("Fuel")).count() >= 3,
        "{reference:?}"
    );

    // Cells parked on the old build at a host call, at fuel parks and at the
    // second host call: snapshot, then the session goes away.
    let mut parked = Vec::new();
    for k in [1, 2, 4, reference.len() - 1] {
        let (mut s, out, p) = started(&rt);
        let mut steps = Vec::new();
        let pending = drive(&mut s, p, &mut steps, Some(k));
        let image = s.snapshot(None).unwrap();
        assert_eq!(image_build_id(&image), Some(old));
        parked.push((image, pending, steps, out.lock().unwrap().clone()));
    }

    // The upgrade: the new build becomes current; the old stays loaded.
    let new = rt.load(&new_guest()).unwrap();
    assert_ne!(new, old, "the two guests must be different builds");
    assert_eq!(rt.current_build(), new);
    assert_eq!(rt.builds(), [old, new]);
    // Native and wasm builds of the same source and features share the id.
    assert_eq!(*micro_monty::build_id(), new);

    // New sessions run on the new build, which lacks the old one's uuid.
    let mut fresh = rt.session(&OPTIONS).unwrap();
    assert_eq!(fresh.build_id().unwrap(), new);
    let p = fresh.start("import uuid", "new.py").unwrap();
    assert!(
        matches!(&p, Progress::Error(e) if e.type_name == "ModuleNotFoundError"),
        "{p:?}"
    );
    let p = fresh.start("sum(range(10))", "new.py").unwrap();
    assert!(
        matches!(&p, Progress::Complete(Some(c)) if c.repr == "45"),
        "{p:?}"
    );
    let new_image = fresh.snapshot(None).unwrap();

    // The old snapshots resume on the old module, with identical output.
    for (image, pending, before, out_before) in &parked {
        let mut s = rt.restore(image, &OPTIONS).unwrap();
        assert_eq!(s.build_id().unwrap(), old);
        let out = output(&mut s);
        let mut after = Vec::new();
        drive(&mut s, pending.clone(), &mut after, None);
        let mut steps = before[..before.len() - 1].to_vec();
        steps.extend(after);
        assert_eq!(steps, reference);
        assert_eq!(
            format!("{out_before}{}", out.lock().unwrap()),
            reference_out
        );
    }
    // The new build's images restore on the new module.
    let mut s = rt.restore(&new_image, &OPTIONS).unwrap();
    assert_eq!(s.build_id().unwrap(), new);

    // A native host of the new build refuses the old images.
    let engine = Engine::new(EngineConfig::default());
    assert!(matches!(
        engine.restore(&parked[0].0),
        Err(SnapshotError::Incompatible { build_id, .. }) if build_id == old
    ));
    assert!(engine.restore(&new_image).is_ok());

    // Drained: the old build goes; its images are now refused cleanly. The
    // current build cannot be unloaded.
    assert!(!rt.unload(&new));
    assert!(rt.unload(&old));
    assert_eq!(rt.builds(), [new]);
    let refused = rt.restore(&parked[0].0, &OPTIONS).err();
    assert_eq!(refused, Some(Error::UnknownBuild(old)));
    assert!(refused.unwrap().to_string().contains("has not loaded"));
    // Not an image at all: the current module reports it as corrupt.
    assert!(matches!(
        rt.restore(b"not a snapshot", &OPTIONS).err(),
        Some(Error::Snapshot(SnapshotError::Corrupt { .. }))
    ));
}

#[test]
fn an_image_of_an_unloaded_build_is_refused() {
    let old_image = {
        let rt = Runtime::new(&old_guest(), RuntimeConfig::default()).unwrap();
        let (mut s, _out, _p) = started(&rt);
        s.snapshot(None).unwrap()
    };
    let rt = Runtime::new(&new_guest(), RuntimeConfig::default()).unwrap();
    let id = image_build_id(&old_image).unwrap();
    assert_ne!(id, rt.current_build());
    assert_eq!(
        rt.restore(&old_image, &OPTIONS).err(),
        Some(Error::UnknownBuild(id))
    );
    // The runtime and its current build are unaffected.
    let mut s = rt.session(&OPTIONS).unwrap();
    assert!(matches!(
        s.start("1 + 1", "a.py").unwrap(),
        Progress::Complete(Some(c)) if c.repr == "2"
    ));
}
