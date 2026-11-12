//! Hashes what an image's meaning rests on in this crate, for the snapshot
//! build id (`micro_monty::build_id`, docs/spec.md §9.3): the code codec
//! (`encode.rs`; images hold user code in its form), and every `const` and
//! `#[repr]` item (operand meanings such as `BinOp` and the match kinds),
//! so edits to function bodies elsewhere, the compiler's included, keep
//! images restorable. What the compiler makes of code is in the id through
//! the prelude's and bundled modules' bytecode. Written to
//! `layout_hash.rs` as a `u64` literal.

use std::{env, fs, path::Path, path::PathBuf};

fn main() -> Result<(), String> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);
    write_layout_hash(&out_dir, &["src/encode.rs"])
}

// The same four functions are in the build scripts of mm-core,
// mm-compile and mm-engine.

/// Writes `layout_hash.rs`: [`layout_hash`] of `whole` as a `u64`
/// literal.
fn write_layout_hash(out_dir: &Path, whole: &[&str]) -> Result<(), String> {
    let h = layout_hash(whole)?;
    let literal = format!(
        "0x{:04x}_{:04x}_{:04x}_{:04x}",
        h >> 48,
        (h >> 32) & 0xffff,
        (h >> 16) & 0xffff,
        h & 0xffff
    );
    fs::write(out_dir.join("layout_hash.rs"), literal).map_err(|e| e.to_string())
}

/// FNV-1a over the code of each file in `whole` and of every `const` and
/// `#[repr]` item in the other `.rs` files under `src/`: comments, blank
/// space, `const fn` and test modules left out.
fn layout_hash(whole: &[&str]) -> Result<u64, String> {
    println!("cargo:rerun-if-changed=src");
    let mut files = Vec::new();
    rs_files(Path::new("src"), &mut files)?;
    files.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |lines: &[&str]| {
        for line in lines {
            for b in line.bytes().chain(*b"\n") {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
        }
    };
    for name in &files {
        let text = fs::read_to_string(name).map_err(|e| format!("{name}: {e}"))?;
        let lines: Vec<&str> = text
            .lines()
            .map(|l| l.find("//").map_or(l, |at| &l[..at]).trim())
            .filter(|l| !l.is_empty())
            .collect();
        if whole.contains(&name.as_str()) {
            eat(&[name]);
            eat(&lines);
            continue;
        }
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let item = line.starts_with("#[repr(") || {
                let rest = line.strip_prefix("pub ").unwrap_or(line);
                let rest = rest
                    .strip_prefix("pub(")
                    .and_then(|r| r.split_once(") "))
                    .map_or(rest, |(_, r)| r);
                rest.starts_with("const ") && !rest.starts_with("const fn ")
            };
            if item || line.ends_with("mod tests {") {
                let end = item_end(&lines, i);
                if item {
                    eat(&lines[i..end]);
                }
                i = end;
            } else {
                i += 1;
            }
        }
    }
    Ok(hash)
}

/// The index after the last line of the item starting at line `i`: the
/// first line, from there, ending in `;` or `}` with every brace closed.
fn item_end(lines: &[&str], i: usize) -> usize {
    let mut depth = 0i64;
    for (k, line) in lines.iter().enumerate().skip(i) {
        // Braces in string and char literals are not the item's.
        let line = line.replace("'{'", "").replace("'}'", "");
        let (mut chars, mut quoted) = (line.chars(), false);
        while let Some(c) = chars.next() {
            match c {
                '\\' if quoted => {
                    chars.next();
                }
                '"' => quoted = !quoted,
                '{' if !quoted => depth += 1,
                '}' if !quoted => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 && (line.ends_with(';') || line.ends_with('}')) {
            return k + 1;
        }
    }
    lines.len()
}

/// Appends the `.rs` files under `dir` but test files, as `a/b.rs`.
fn rs_files(dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if path.is_dir() {
            rs_files(&path, out)?;
        } else if path.extension().is_some_and(|x| x == "rs") && !name.ends_with("_tests.rs") {
            out.push(name);
        }
    }
    Ok(())
}
