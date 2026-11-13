//! `cargo xtask handoffs [--check]`: the native-to-Python hand-off sites
//! (`Vm::need_py()` calls, docs/spec.md §5.1) per file and enclosing
//! function, written to `xtask/handoff-sites.txt`; `--check` (in `cargo
//! xtask check`) fails when the file is not what the tree gives. No line
//! numbers, so unrelated edits do not churn it; the cleanup lane's "one
//! chunked path per builtin" counts down here.
//!
//! The enclosing function is the nearest `fn` above the call at a smaller
//! indentation that has not closed (its closing `}` at its own
//! indentation): rustfmt's layout, which `check` enforces. A closure's
//! call counts for the function around it.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use crate::workspace_root;

const SITES: &str = "xtask/handoff-sites.txt";
const CALL: &str = "need_py()";

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let check = match args {
        [] => false,
        [a] if a == "--check" => true,
        _ => return Err("usage: cargo xtask handoffs [--check]".to_owned()),
    };
    let root = workspace_root();
    let mut files = Vec::new();
    for entry in
        fs::read_dir(root.join("crates")).map_err(|e| format!("cannot read crates: {e}"))?
    {
        let src = entry.map_err(|e| e.to_string())?.path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files)?;
        }
    }
    files.sort();
    let mut sites: BTreeMap<(String, String), usize> = BTreeMap::new();
    for path in &files {
        let text =
            fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        for (function, count) in scan(&text) {
            *sites.entry((rel.clone(), function)).or_default() += count;
        }
    }
    let total: usize = sites.values().sum();
    let file_count = sites
        .keys()
        .map(|(f, _)| f.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let mut text = format!(
        "# Native-to-Python hand-off sites (`{CALL}` calls): file, enclosing\n\
         # function, count. Written by `cargo xtask handoffs`; checked in\n\
         # `cargo xtask check`. {total} sites in {} functions in {file_count} files.\n",
        sites.len()
    );
    for ((file, function), count) in &sites {
        let _ = writeln!(text, "{file}\t{function}\t{count}");
    }
    let out = root.join(SITES);
    if check {
        let have = fs::read_to_string(&out).unwrap_or_default();
        if have != text {
            return Err(format!(
                "{SITES} is out of date ({total} `{CALL}` sites in the tree): run \
                 `cargo xtask handoffs` and commit it"
            ));
        }
        println!("handoffs: {SITES} matches the tree ({total} sites)");
        return Ok(());
    }
    fs::write(&out, text).map_err(|e| format!("cannot write {SITES}: {e}"))?;
    println!(
        "handoffs: {total} sites in {} functions in {file_count} files",
        sites.len()
    );
    Ok(())
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// The `need_py()` calls of one file, by enclosing function (`?` outside
/// any). Calls in `//` comments and the definition do not count.
fn scan(text: &str) -> BTreeMap<String, usize> {
    // Open functions: (indentation, name).
    let mut open: Vec<(usize, String)> = Vec::new();
    let mut counts = BTreeMap::new();
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let body = line.trim_start();
        if body.starts_with('}') {
            open.retain(|(at, _)| *at < indent);
        }
        let code = body.split("//").next().unwrap_or("");
        if let Some(name) = fn_name(code) {
            open.retain(|(at, _)| *at < indent);
            // A one-line `fn ... { ... }` closes on its own line.
            if !code.trim_end().ends_with('}') {
                open.push((indent, name.to_owned()));
            }
            continue;
        }
        let n = code.matches(CALL).count();
        if n > 0 {
            let function = open
                .iter()
                .rev()
                .find(|(at, _)| *at < indent)
                .map_or("?", |(_, name)| name.as_str());
            *counts.entry(function.to_owned()).or_default() += n;
        }
    }
    counts
}

/// The name a line declares with `fn NAME`, if it starts a function.
fn fn_name(code: &str) -> Option<&str> {
    let mut words = code.split_whitespace();
    loop {
        match words.next()? {
            "fn" => break,
            w if w == "pub"
                || w.starts_with("pub(")
                || matches!(w, "const" | "async" | "unsafe" | "extern" | "\"C\"") => {}
            _ => return None,
        }
    }
    let rest = words.next()?;
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}
