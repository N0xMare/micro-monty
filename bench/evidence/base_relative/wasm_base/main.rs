//! Writes the fresh-session image (`Session::new`, then `snapshot`) of the
//! native facade and of each mm-wasm v1 guest given, for byte comparison.
//! All are default-feature builds, so they share one build id.
//!
//! `wasm-base OUT_DIR GUEST.wasm...`

use micro_monty::{Engine, EngineConfig, Session};
use mm_wasm_host::{Runtime, RuntimeConfig, SessionOptions};
use std::path::Path;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let out = Path::new(args.get(1).ok_or("OUT_DIR")?);
    let engine = Engine::new(EngineConfig::default());
    let native = Session::new(&engine)
        .map_err(|e| e.traceback)?
        .snapshot()
        .map_err(|e| e.to_string())?;
    std::fs::write(out.join("native-default.mmsnap"), &native).map_err(|e| e.to_string())?;
    println!(
        "native: {} bytes, build id {:02x?}",
        native.len(),
        &micro_monty::build_id()[..4]
    );
    for (i, guest) in args[2..].iter().enumerate() {
        let rt = Runtime::new(Path::new(guest), RuntimeConfig::default())
            .map_err(|e| format!("{e:?}"))?;
        let mut s = rt
            .session(&SessionOptions::default())
            .map_err(|e| format!("{e:?}"))?;
        let image = s.snapshot(None).map_err(|e| format!("{e:?}"))?;
        std::fs::write(out.join(format!("guest{i}.mmsnap")), &image).map_err(|e| e.to_string())?;
        println!(
            "{guest}: {} bytes, identical to native: {}",
            image.len(),
            image == native
        );
    }
    Ok(())
}
