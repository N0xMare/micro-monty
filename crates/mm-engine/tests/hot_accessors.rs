//! The heap accessors' and the small `Vm` helpers' plain names
//! (`Vm::slot`, `raw`, `hx`, `backing`, `alloc_tuple`, ...; [`ACCESSORS`])
//! are out of line; their `_hot` forms are inline, and inlined copies of
//! them cost about 150 KB of `hello.wasm` when used everywhere [M]. Only the functions in
//! `hot_accessors.txt`, the ones the dispatch loop inlines or calls and the
//! natives with a measurable share of some bench program's instructions,
//! may call a `_hot` form. A new hot function is added to the list with a
//! bench Ir table showing it needs the inline form; a function that no
//! longer calls one leaves the list.
#![allow(clippy::expect_used, reason = "tests")]

use std::collections::BTreeSet;

const ACCESSORS: [&str; 21] = [
    "slot",
    "vslot",
    "raw",
    "set_slot",
    "set_raw",
    "str_ref",
    "hx",
    "backing",
    "raw_of",
    "set_raw_of",
    "unbx",
    "alloc_tuple",
    "dv",
    "type_of_typeobj",
    "str_bytes",
    "intern",
    "obj_len",
    "list_len",
    "new_dict",
    "list_parts",
    "meta_of",
];

/// The name of the `fn` a line starts, if it starts one.
fn fn_name(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    let rest = rest
        .strip_prefix("pub(crate) ")
        .or_else(|| rest.strip_prefix("pub "))
        .unwrap_or(rest);
    let rest = rest.strip_prefix("const ").unwrap_or(rest);
    let rest = rest.strip_prefix("fn ")?;
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

/// `file::fn` of every function in `src/` that calls a `_hot` accessor.
fn hot_callers() -> BTreeSet<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let calls: Vec<String> = ACCESSORS.iter().map(|a| format!(".{a}_hot(")).collect();
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir(dir).expect("src/") {
        let path = entry.expect("a directory entry").path();
        let file = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a file name")
            .to_owned();
        if path.extension().is_none_or(|e| e != "rs") || file.ends_with("_tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a source file");
        let mut current = None;
        for line in text.lines() {
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            if let Some(name) = fn_name(line) {
                current = Some(name.to_owned());
            }
            if calls.iter().any(|c| line.contains(c.as_str())) {
                let f = current.as_deref().unwrap_or("<top>");
                // The out-of-line form and the other `_hot` forms are the
                // definitions themselves.
                if ACCESSORS
                    .iter()
                    .any(|a| f == *a || f.strip_suffix("_hot") == Some(a))
                {
                    continue;
                }
                out.insert(format!("{file}::{f}"));
            }
        }
    }
    out
}

#[test]
fn only_listed_functions_call_the_inline_accessors() {
    let listed: BTreeSet<String> = include_str!("hot_accessors.txt")
        .lines()
        .map(str::to_owned)
        .collect();
    let found = hot_callers();
    let unlisted: Vec<_> = found.difference(&listed).collect();
    assert!(
        unlisted.is_empty(),
        "these functions call a `_hot` accessor without being listed in \
         tests/hot_accessors.txt; use the plain (out-of-line) name, or list \
         the function with a bench Ir table that shows it needs the inline \
         form: {unlisted:?}"
    );
    let stale: Vec<_> = listed.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "listed in tests/hot_accessors.txt but calling no `_hot` accessor; \
         remove them: {stale:?}"
    );
}
