//! Every use of a generator's `YPC` slot in the engine's source is one this
//! list names: a reader masks `g::CLEARED` (bit 63, set once a collection
//! cleared the suspended copy's dead registers) within a few lines, and a
//! writer is a suspension, which writes the slot whole and so resets the
//! bit. A new use fails here until it is added with its role.

#![allow(clippy::panic, clippy::unwrap_used, reason = "test")]

use std::{fs, path::Path};

/// (file, a text the line holds, the role).
const USES: &[(&str, &str, &str)] = &[
    ("generator.rs", "pub const YPC", "the slot"),
    (
        "generator.rs",
        "word(w.get(g::YPC as usize))",
        "read, gen_enter",
    ),
    ("generator.rs", "w[g::YPC as usize]", "read, gen_enter_fast"),
    (
        "generator.rs",
        "set_raw_guarded(r, g::YPC, pc as u64)",
        "write, gen_yield_fast suspends",
    ),
    (
        "generator.rs",
        "self.gen_raw(gn, g::YPC)",
        "read, gen_throw",
    ),
    (
        "generator.rs",
        "raw(g::YPC)",
        "read with the bit, clear_saved",
    ),
    (
        "generator.rs",
        "set_raw(r, g::YPC, ypc | g::CLEARED)",
        "write, clear_saved sets the bit",
    ),
    (
        "aio.rs",
        "self.gen_raw(gn, g::YPC)",
        "read, unfinished_asyncgens",
    ),
    (
        "snapshot.rs",
        ".get_raw(r, g::YPC)",
        "read, restore's generators check",
    ),
];

/// Lines from a use within which a reader must mask the bit.
const NEAR: usize = 12;

#[test]
fn every_ypc_use_is_listed_and_every_reader_masks_the_cleared_bit() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for entry in fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.extension().is_none_or(|x| x != "rs") || name.ends_with("_tests.rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if !code.contains("YPC") {
                continue;
            }
            let Some(&(_, _, role)) = USES
                .iter()
                .find(|(f, text, _)| *f == name && code.contains(text))
            else {
                panic!("{name}:{}: an unlisted use of YPC: {}", i + 1, line.trim());
            };
            if role.starts_with("read,") {
                let masked = lines[i..(i + NEAR).min(lines.len())]
                    .iter()
                    .any(|l| l.contains("!g::CLEARED"));
                assert!(masked, "{name}:{}: {role} does not mask g::CLEARED", i + 1);
            }
            found.push((name.clone(), role));
        }
    }
    for &(file, _, role) in USES {
        assert!(
            found.iter().any(|(f, r)| f == file && *r == role),
            "{file}: no use found for `{role}`; update the list"
        );
    }
    // The other suspension, `gen_yield`, writes STATE and YPC as one
    // pair of raw words: YPC whole.
    let generator = fs::read_to_string(src.join("generator.rs")).unwrap();
    assert!(generator.contains("if let [state, ypc, ..] = raw {"));
    assert!(generator.contains("*ypc = pc as u64;"));
}
