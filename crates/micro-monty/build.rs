//! Hashes the snapshot header codec (`src/image.rs`) for the snapshot
//! build id (`micro_monty::build_id`, docs/spec.md §9.3): its code, with
//! comments and blank space left out, FNV-1a, written to `layout_hash.rs`
//! as a `u64` literal. The engine crates hash their own codecs and
//! layouts.

use std::{env, fs, path::PathBuf};

fn main() -> Result<(), String> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);
    let file = "src/image.rs";
    println!("cargo:rerun-if-changed={file}");
    let text = fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let code = text
        .lines()
        .map(|l| l.find("//").map_or(l, |at| &l[..at]).trim())
        .filter(|l| !l.is_empty());
    for line in code {
        for b in line.bytes().chain(*b"\n") {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    let literal = format!(
        "0x{:04x}_{:04x}_{:04x}_{:04x}",
        hash >> 48,
        (hash >> 32) & 0xffff,
        (hash >> 16) & 0xffff,
        hash & 0xffff
    );
    fs::write(out_dir.join("layout_hash.rs"), literal).map_err(|e| e.to_string())
}
