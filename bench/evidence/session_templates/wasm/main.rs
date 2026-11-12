//! Session templates through `mm-wasm-host` (precompiled guest, one session
//! per instance): a new instance plus the import cell, against a new
//! instance restoring the template image `tpl image` wrote natively.
//!
//! `wasm-tpl GUEST.wasm IMAGE_DIR REPS` prints one JSON line per set: wall
//! ns of the first operation and the median of REPS, guest memory per
//! instance, whether the image the guest writes after the same imports is
//! byte-identical to the native one, and a cell's output after a restore.

use mm_wasm_host::{Progress, Runtime, RuntimeConfig, Session, SessionOptions};
use serde_json::json;
use std::{path::Path, time::Instant};

/// The import cells of `tpl` (bench/evidence/session_templates/main.rs):
/// the modules load and no names stay in the globals.
const SETS: [(&str, &str); 5] = [
    ("empty", ""),
    ("json", "import json\ndel json\n"),
    (
        "codemode",
        "import json, asyncio, re, collections, datetime\ndel json, asyncio, re, collections, datetime\n",
    ),
    (
        "codemode-tz",
        "import json, asyncio, re, collections, datetime, zoneinfo, calendar\ndel json, asyncio, re, collections, datetime, zoneinfo, calendar\n",
    ),
    (
        "heavy",
        "import decimal, statistics, zoneinfo, html.parser, tomllib\ndel decimal, statistics, zoneinfo, html, tomllib\n",
    ),
];

fn zones(s: &mut Session) {
    s.set_zone_source(Some(Box::new(|key: &str| {
        if key.starts_with('/') || key.split('/').any(|p| p == ".." || p.is_empty()) {
            return None;
        }
        std::fs::read(Path::new("/usr/share/zoneinfo").join(key))
            .ok()
            .filter(|b| b.starts_with(b"TZif"))
    })));
}

fn fresh(rt: &Runtime, src: &str) -> Result<Session, String> {
    let mut s = rt
        .session(&SessionOptions::default())
        .map_err(|e| format!("{e:?}"))?;
    zones(&mut s);
    if !src.is_empty() {
        match s.start(src, "<cell>").map_err(|e| format!("{e:?}"))? {
            Progress::Complete(_) => {}
            other => return Err(format!("imports: {other:?}")),
        }
    }
    Ok(s)
}

fn main() -> Result<(), String> {
    let a: Vec<String> = std::env::args().collect();
    let (guest, dir) = (Path::new(&a[1]), Path::new(&a[2]));
    let reps: usize = a[3].parse().map_err(|_| "REPS")?;
    let t = Instant::now();
    let rt = Runtime::new(guest, RuntimeConfig::default()).map_err(|e| format!("{e:?}"))?;
    let load_ns = t.elapsed().as_nanos();
    for (set, src) in SETS {
        let image = std::fs::read(dir.join(format!("{set}.mmsnap"))).map_err(|e| e.to_string())?;
        let time =
            |f: &dyn Fn() -> Result<Session, String>| -> Result<(u128, u128, usize), String> {
                let mut ns = Vec::new();
                let mut mem = 0;
                for _ in 0..=reps {
                    let t = Instant::now();
                    let s = f()?;
                    ns.push(t.elapsed().as_nanos());
                    mem = s.memory_bytes();
                }
                let first = ns.remove(0);
                ns.sort_unstable();
                Ok((first, ns[ns.len() / 2], mem))
            };
        let (ff, fm, fmem) = time(&|| fresh(&rt, src))?;
        let (rf, rm, rmem) = time(&|| {
            rt.restore(&image, &SessionOptions::default())
                .map_err(|e| format!("{e:?}"))
        })?;
        let mut s = fresh(&rt, src)?;
        let guest_image = s.snapshot(None).map_err(|e| format!("{e:?}"))?;
        let mut r = rt
            .restore(&image, &SessionOptions::default())
            .map_err(|e| format!("{e:?}"))?;
        let cell = r
            .start(
                "import json, datetime\njson.dumps({'d': str(datetime.date(2026, 3, 15))})",
                "<probe>",
            )
            .map_err(|e| format!("{e:?}"))?;
        let cell = match cell {
            Progress::Complete(Some(c)) => c.repr,
            other => format!("{other:?}"),
        };
        println!(
            "{}",
            json!({"set": set, "guest": guest.display().to_string(), "runtime_new_ns": load_ns,
                "fresh_first_ns": ff, "fresh_median_ns": fm, "fresh_guest_memory": fmem,
                "restore_first_ns": rf, "restore_median_ns": rm, "restore_guest_memory": rmem,
                "guest_image_equals_native": guest_image == image, "cell_after_restore": cell})
        );
    }
    Ok(())
}
