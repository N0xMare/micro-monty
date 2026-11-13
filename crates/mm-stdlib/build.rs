//! Compiles the bundled Python modules (`python/<name>.py`, packages as
//! `python/<name>/__init__.py`) to bytecode at build time, as mm-engine's
//! build script compiles the prelude, and writes the module table
//! (`modules.rs`): one entry per module whose `mod-<top-level name>`
//! feature is on (docs/spec.md §12), in sections
//! (`Program::encode_sections`) packed against codes the entries share
//! per section, with the strings several of them use stored once.

use mm_compile::ast::{Const, Expr, ExprKind, Stmt, StmtKind};
use mm_compile::bytecode::Program;
use mm_compile::section::COUNT;
use std::{
    collections::BTreeMap, collections::BTreeSet, env, fmt::Write as _, fs, path::Path,
    path::PathBuf,
};

#[allow(
    clippy::too_many_lines,
    reason = "one pass: compile, pack, check and table every module"
)]
fn main() -> Result<(), String> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);
    // A new or removed module changes the table; `collect` names each
    // module file it reads.
    println!("cargo:rerun-if-changed=python");
    let mut found = Vec::new();
    collect(Path::new("python"), "", &mut found)?;
    found.sort();
    check_feature_graph(&found)?;
    let mut table = String::new();
    let mut programs = Vec::new();
    // Only the enabled modules are packed, against codes and shared strings
    // fitted to them (the features are part of the build id).
    let enabled: Vec<&(String, PathBuf)> = found
        .iter()
        .filter(|(name, _)| {
            let feature = format!("CARGO_FEATURE_MOD_{}", feature(name).to_uppercase());
            env::var_os(feature.replace('-', "_")).is_some()
        })
        .collect();
    for (_, path) in &enabled {
        programs.push(compile_module(path)?);
    }
    let owned = shared_strings(&programs);
    let shared: Vec<&str> = owned.iter().map(String::as_str).collect();
    let (text, ends) = mm_compile::SharedStrings::build(&shared)
        .ok_or_else(|| String::from("shared strings: more than 64 KiB"))?;
    fs::write(out_dir.join("strings.txt"), &text).map_err(|e| format!("strings.txt: {e}"))?;
    fs::write(out_dir.join("string_ends.bin"), &ends)
        .map_err(|e| format!("string_ends.bin: {e}"))?;
    // Each section packed against one set of codes for every module.
    let sections: Vec<[Vec<u8>; COUNT]> = programs
        .iter()
        .map(|p| p.encode_sections(&shared))
        .collect();
    let mut packed: Vec<Vec<Vec<u8>>> = vec![Vec::new(); programs.len()];
    let mut headers = Vec::new();
    for k in 0..COUNT {
        let parts: Vec<&[u8]> = sections.iter().map(|s| s[k].as_slice()).collect();
        let (header, out) = mm_compile::pack::pack_shared(&parts);
        let file = format!("section{k}.bin");
        fs::write(out_dir.join(&file), &header).map_err(|e| format!("{file}: {e}"))?;
        headers.push(header);
        for (m, part) in packed.iter_mut().zip(out) {
            m.push(part);
        }
    }
    if programs.is_empty() {
        // No module refers to it.
        table.push_str("#[allow(dead_code, reason = \"no module is enabled\")]\n");
    }
    table.push_str("static PACKING: mm_engine::Packing = mm_engine::Packing {\n    headers: [\n");
    for k in 0..COUNT {
        let _ = writeln!(
            table,
            "        include_bytes!(concat!(env!(\"OUT_DIR\"), \"/section{k}.bin\")),"
        );
    }
    table.push_str(
        "    ],\n    text: include_str!(concat!(env!(\"OUT_DIR\"), \"/strings.txt\")),\n    \
         ends: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/string_ends.bin\")),\n};\n",
    );
    table.push_str("/// Every bundled Python module this build enables.\n");
    table.push_str("#[allow(clippy::unreadable_literal, reason = \"generated\")]\n");
    table.push_str("pub static MODULES: &[mm_engine::StdModule] = &[\n");
    let shared_strings = mm_compile::SharedStrings::new(&text, &ends);
    // With `host-bundle`, the modules' code is the host's: one blob, each
    // module at its offset (`StdModule::external`).
    let host_bundle = env::var_os("CARGO_FEATURE_HOST_BUNDLE").is_some();
    let mut bundle: Vec<u8> = Vec::new();
    // The build id covers what every module's code is unpacked with (the
    // section headers and the shared strings), then each module's name and
    // code (`CODE_HASH`).
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for header in &headers {
        hash = fnv1a(hash, &[header]);
    }
    hash = fnv1a(hash, &[text.as_bytes(), &ends]);
    for (((name, _), program), parts) in enabled.iter().zip(&programs).zip(&packed) {
        let file = format!("{name}.bin");
        let mut code = Vec::new();
        for part in &parts[..COUNT - 1] {
            let mut n = part.len();
            while n >= 0x80 {
                code.push(u8::try_from(n & 0x7f).unwrap_or(0) | 0x80);
                n >>= 7;
            }
            code.push(u8::try_from(n).unwrap_or(0));
        }
        let mut unpacked: Vec<Vec<u8>> = Vec::new();
        for (k, part) in parts.iter().enumerate() {
            code.extend_from_slice(part);
            let bytes =
                mm_compile::pack::unpack_shared_bounded(&headers[k], part, &mut { u64::MAX })
                    .map_err(|e| format!("{file}: section {k} does not unpack: {e:?}"))?;
            unpacked.push(bytes);
        }
        let refs: [&[u8]; COUNT] = std::array::from_fn(|k| unpacked[k].as_slice());
        let decoded = Program::decode_sections(&refs, shared_strings, &mut { u64::MAX })
            .map_err(|e| format!("{file}: does not decode: {e:?}"))?;
        if decoded.codes != program.codes || decoded.entry != program.entry {
            return Err(format!("{file}: packing does not round-trip"));
        }
        hash = fnv1a(hash, &[name.as_bytes(), &code]);
        fs::write(out_dir.join(&file), &code).map_err(|e| format!("{file}: {e}"))?;
        if host_bundle {
            let _ = writeln!(
                table,
                "    mm_engine::StdModule {{\n        name: \"{name}\",\n        code: &[],\n        packing: Some(&PACKING),\n        external: ({}, {}),\n    }},",
                bundle.len(),
                code.len()
            );
            bundle.extend_from_slice(&code);
        } else {
            let _ = writeln!(
                table,
                "    mm_engine::StdModule {{\n        name: \"{name}\",\n        code: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{file}\")),\n        packing: Some(&PACKING),\n    }},"
            );
        }
    }
    table.push_str("];\n");
    let _ = writeln!(
        table,
        "/// FNV-1a of the shared packing (section headers, strings) and of\n\
         /// every enabled module's name and packed code, in order, for the\n\
         /// snapshot build id (computed at build time; the same with and\n\
         /// without `host-bundle`).\n\
         pub const CODE_HASH: u64 = 0x{:04x}_{:04x}_{:04x}_{:04x};",
        hash >> 48,
        (hash >> 32) & 0xffff,
        (hash >> 16) & 0xffff,
        hash & 0xffff
    );
    if host_bundle {
        fs::write(out_dir.join("stdlib_bundle.bin"), &bundle)
            .map_err(|e| format!("bundle: {e}"))?;
    }
    table.push_str("/// Every bundled Python module, enabled in this build or not.\n");
    table.push_str("pub static ALL_MODULES: &[&str] = &[\n");
    for (name, _) in &found {
        let _ = writeln!(table, "    \"{name}\",");
    }
    table.push_str("];\n");
    fs::write(out_dir.join("modules.rs"), table).map_err(|e| format!("modules.rs: {e}"))
}

/// The module at `path`, compiled without what bundled frames never use.
fn compile_module(path: &Path) -> Result<Program, String> {
    println!("cargo:rerun-if-changed={}", path.display());
    let src = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut module = mm_parse::parse(&src).map_err(|e| format!("{}: {e:?}", path.display()))?;
    strip_docs(&mut module.body);
    let mut program = mm_compile::compile(
        &module,
        mm_compile::Options {
            last_expr_value: false,
        },
    )
    .map_err(|e| format!("{}: {e:?}", path.display()))?;
    // Bundled frames never appear in tracebacks: drop what only they use.
    for code in program.codes.iter_mut().map(std::sync::Arc::make_mut) {
        code.positions.clear();
        code.reg_names.clear();
        code.doc = None;
        code.first_line = 0;
    }
    Ok(program)
}

/// The strings three or more of `programs` use, stored once, most used
/// first (docs/spec.md §6).
fn shared_strings(programs: &[Program]) -> Vec<String> {
    let mut users: BTreeMap<String, usize> = BTreeMap::new();
    for p in programs {
        for s in p.strings() {
            *users.entry(s).or_default() += 1;
        }
    }
    let mut shared: Vec<(String, usize)> = users.into_iter().filter(|&(_, n)| n >= 3).collect();
    shared.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    shared.into_iter().map(|(s, _)| s).collect()
}

/// Fails unless every import a bundled module runs when it is imported is
/// enabled by the module's own `mod-*` feature, directly or through the
/// features it lists in `Cargo.toml`, so that no build the manifest allows
/// has a module that cannot be imported. It reads every module, enabled in
/// this build or not, and the manifest's `[features]` table (also in the
/// form `cargo package` writes). An import runs at import time unless it
/// is in a function body, under `if TYPE_CHECKING:` or `if False:`, or in
/// a `try` body with an `except ImportError`/`ModuleNotFoundError`
/// handler (an optional import). Imports inside functions are not checked
/// (docs/spec.md §12 lists them).
fn check_feature_graph(found: &[(String, PathBuf)]) -> Result<(), String> {
    println!("cargo:rerun-if-changed=Cargo.toml");
    let manifest = fs::read_to_string("Cargo.toml").map_err(|e| format!("Cargo.toml: {e}"))?;
    let graph = features_table(&manifest);
    let mut missing = Vec::new();
    for (name, path) in found {
        let own = format!("mod-{}", feature(name));
        let Some(direct) = graph.get(&own) else {
            missing.push(format!("module `{name}`: no feature `{own}` in Cargo.toml"));
            continue;
        };
        let mut enabled = BTreeSet::new();
        let mut todo: Vec<&str> = direct.iter().map(String::as_str).collect();
        while let Some(f) = todo.pop() {
            if enabled.insert(f)
                && let Some(more) = graph.get(f)
            {
                todo.extend(more.iter().map(String::as_str));
            }
        }
        let src = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let module = mm_parse::parse(&src).map_err(|e| format!("{}: {e:?}", path.display()))?;
        let mut imports = Vec::new();
        import_time_imports(&module.body, &mut imports);
        for import in imports {
            let needed = match import.split('.').next().unwrap_or(&import) {
                "sys" | "math" | "cmath" | "_mm" | "__future__" => continue,
                "asyncio" => String::from("mod-asyncio"),
                "json" => String::from("mod-json"),
                top => {
                    let provider = found
                        .iter()
                        .find(|(m, _)| *m == import)
                        .or_else(|| found.iter().find(|(m, _)| m == top));
                    let Some((m, _)) = provider else {
                        missing.push(format!(
                            "module `{name}` imports `{import}` when it is imported, \
                             and no bundled or engine module provides it"
                        ));
                        continue;
                    };
                    format!("mod-{}", feature(m))
                }
            };
            if needed != own && !enabled.contains(needed.as_str()) {
                missing.push(format!(
                    "module `{name}` imports `{import}` when it is imported, but `{own}` \
                     does not turn on `{needed}`: add \"{needed}\" to `{own}` in \
                     crates/mm-stdlib/Cargo.toml"
                ));
            }
        }
    }
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the mod-* feature graph misses imports:\n  {}",
            missing.join("\n  ")
        ))
    }
}

/// `Cargo.toml`'s `[features]` table: each feature and what it lists.
fn features_table(manifest: &str) -> BTreeMap<String, Vec<String>> {
    let mut section = String::new();
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') && !line.contains('"') {
            inside = line == "[features]";
            continue;
        }
        if inside {
            section.push_str(line);
            section.push(' ');
        }
    }
    let mut table = BTreeMap::new();
    for entry in section.split(']') {
        let Some((key, list)) = entry.split_once('=') else {
            continue;
        };
        let items = list
            .trim()
            .trim_start_matches('[')
            .split(',')
            .map(|item| item.trim().trim_matches('"').to_owned())
            .filter(|item| !item.is_empty())
            .collect();
        table.insert(key.trim().trim_matches('"').to_owned(), items);
    }
    table
}

/// The modules `body` imports when it runs as a module body.
fn import_time_imports(body: &[Stmt], out: &mut Vec<String>) {
    for s in body {
        match &s.kind {
            StmtKind::Import(names) => out.extend(names.iter().map(|a| a.name.clone())),
            StmtKind::ImportFrom(module, _) => out.push(module.clone()),
            StmtKind::ClassDef(c) => import_time_imports(&c.body, out),
            StmtKind::If(test, a, b) => {
                let never = match &test.kind {
                    ExprKind::Name(n) | ExprKind::Attribute(_, n) => n == "TYPE_CHECKING",
                    ExprKind::Const(Const::Bool(false)) => true,
                    _ => false,
                };
                if !never {
                    import_time_imports(a, out);
                }
                import_time_imports(b, out);
            }
            StmtKind::While(_, a, b) | StmtKind::For(_, _, a, b, _) => {
                import_time_imports(a, out);
                import_time_imports(b, out);
            }
            StmtKind::With(_, a, _) => import_time_imports(a, out),
            StmtKind::Match(_, cases) => {
                for c in cases {
                    import_time_imports(&c.body, out);
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            } => {
                let optional = handlers.iter().any(|h| {
                    let mut names = Vec::new();
                    match h.typ.as_ref().map(|t| &t.kind) {
                        Some(ExprKind::Name(n)) => names.push(n),
                        Some(ExprKind::Tuple(items)) => {
                            for item in items {
                                if let ExprKind::Name(n) = &item.kind {
                                    names.push(n);
                                }
                            }
                        }
                        _ => {}
                    }
                    names
                        .iter()
                        .any(|n| *n == "ImportError" || *n == "ModuleNotFoundError")
                });
                if !optional {
                    import_time_imports(body, out);
                }
                for h in handlers {
                    import_time_imports(&h.body, out);
                }
                import_time_imports(orelse, out);
                import_time_imports(finalbody, out);
            }
            _ => {}
        }
    }
}

/// The `mod-*` feature that enables module `name`.
fn feature(name: &str) -> &str {
    // A submodule large enough to need its own feature.
    match name {
        "html.entities" => "html-entities",
        "_markupbase" => "html",
        "token" => "tokenize",
        _ => name.split('.').next().unwrap_or(name),
    }
}

/// Drops docstrings (the module's, classes' and functions'), so bundled
/// objects have `__doc__` None and their text is not embedded.
fn strip_docs(body: &mut [Stmt]) {
    if let Some(Stmt {
        kind:
            StmtKind::Expr(Expr {
                kind: ExprKind::Const(Const::Str(_)),
                ..
            }),
        pos,
    }) = body.first()
    {
        body[0] = Stmt {
            kind: StmtKind::Pass,
            pos: *pos,
        };
    }
    strip_nested(body);
}

/// [`strip_docs`] in a block that has no docstring of its own.
fn strip_nested(body: &mut [Stmt]) {
    for s in body {
        match &mut s.kind {
            StmtKind::FunctionDef(f) => strip_docs(&mut f.body),
            StmtKind::ClassDef(c) => strip_docs(&mut c.body),
            StmtKind::If(_, a, b) | StmtKind::While(_, a, b) | StmtKind::For(_, _, a, b, _) => {
                strip_nested(a);
                strip_nested(b);
            }
            StmtKind::With(_, a, _) => strip_nested(a),
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            } => {
                strip_nested(body);
                for h in handlers {
                    strip_nested(&mut h.body);
                }
                strip_nested(orelse);
                strip_nested(finalbody);
            }
            _ => {}
        }
    }
}

/// The modules under `dir` (a package named `prefix`, or the root).
fn collect(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for e in entries {
        let path = e.map_err(|e| e.to_string())?.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        if path.is_dir() {
            collect(&path, &format!("{prefix}{stem}."), out)?;
        } else if path.extension().is_some_and(|x| x == "py") {
            let name = if stem == "__init__" {
                prefix.trim_end_matches('.').to_owned()
            } else {
                format!("{prefix}{stem}")
            };
            out.push((name, path));
        }
    }
    Ok(())
}

/// 64-bit FNV-1a, continuing from `hash`, over `parts`, each preceded by
/// its length (u64, little-endian).
fn fnv1a(mut hash: u64, parts: &[&[u8]]) -> u64 {
    for part in parts {
        for &b in (part.len() as u64).to_le_bytes().iter().chain(*part) {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}
