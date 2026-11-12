//! Compiles the prelude (`src/prelude.py`) at build time; the engine loads
//! the encoded code without parsing (docs/spec.md §5, §12).
//!
//! The prelude is split into units, each loaded into a session on the first
//! lookup of its name (`src/prelude.rs`): top-level `def`s with literal
//! defaults, classes whose bodies are methods and literals, instances of
//! such classes made with literal arguments, and `NAME = OTHER` aliases.
//! The other top-level statements (the core) stay one module body that runs
//! at session start.
//!
//! Also hashes what an image's meaning rests on in this crate
//! (`LAYOUT_HASH`, for the snapshot build id, docs/spec.md §9.3): the
//! VM-section codec, and every `const` and `#[repr]` item (object, frame,
//! iterator, continuation and native-step layouts, the matcher's
//! programs), so edits to function bodies elsewhere keep images
//! restorable. mm-core and mm-compile hash their own.

use mm_compile::ast::{Const, Expr, ExprKind, Module, Params, Stmt, StmtKind, UnaryOp};
use mm_compile::bytecode::{Code, Program, instr_len, op, opcode, operand};
use std::{collections::BTreeMap, env, fmt::Write as _, fs, path::Path, path::PathBuf};

/// Unpacked bytes per chunk of units, about: a lookup unpacks one chunk.
const CHUNK: usize = 4096;

/// Parts per chunk: the nine program sections, then the unit records.
const PARTS: usize = mm_compile::section::COUNT + 1;

enum Kind {
    /// A function: its code index, defaults and keyword defaults (encoded
    /// literals).
    Func(usize, Vec<Vec<u8>>, Vec<(String, Vec<u8>)>),
    /// A class: its bases, `__module__`, namespace entries in body order
    /// (encoded) and attributes set after the body.
    Class(
        Vec<String>,
        String,
        Vec<(String, Vec<u8>)>,
        Vec<(String, Vec<u8>)>,
    ),
    /// An instance of class unit `class` with these attributes, in order.
    Inst(String, Vec<(String, Vec<u8>)>),
    /// Another unit's value.
    Alias(String),
    /// A literal (encoded).
    Value(Vec<u8>),
}

struct Unit {
    kind: Kind,
    /// The encoded program (functions and classes; empty otherwise).
    program: Vec<u8>,
    /// Its line in prelude.py (chunks keep source order).
    line: u32,
}

/// Appends `n` as LEB128.
fn leb(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// Reads a LEB128 length.
fn read_leb(r: &mut &[u8]) -> Result<usize, String> {
    let mut v = 0usize;
    for shift in (0..64).step_by(7) {
        let (&b, rest) = r.split_first().ok_or("prelude records: truncated")?;
        *r = rest;
        v |= usize::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err(String::from("prelude records: bad length"))
}

/// Appends `s`: its length, then its bytes.
fn put_str(out: &mut Vec<u8>, s: &str) {
    leb(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

/// Appends `(name, encoded value)` pairs, counted.
fn put_pairs(out: &mut Vec<u8>, v: &[(String, Vec<u8>)]) {
    leb(out, v.len() as u64);
    for (k, x) in v {
        put_str(out, k);
        out.extend_from_slice(x);
    }
}

/// A unit's record, as `src/prelude.rs` reads it: its length, the length of
/// each of its program's sections (all 0 without a program), then what its
/// kind needs.
fn record(u: &Unit, sections: &[Vec<u8>; mm_compile::section::COUNT]) -> Vec<u8> {
    let mut out = Vec::new();
    for s in sections {
        leb(&mut out, s.len() as u64);
    }
    match &u.kind {
        Kind::Func(func, d, kw) => {
            leb(&mut out, *func as u64);
            leb(&mut out, d.len() as u64);
            for x in d {
                out.extend_from_slice(x);
            }
            put_pairs(&mut out, kw);
        }
        Kind::Class(bases, module, ns, after) => {
            leb(&mut out, bases.len() as u64);
            for b in bases {
                put_str(&mut out, b);
            }
            put_str(&mut out, module);
            put_pairs(&mut out, ns);
            put_pairs(&mut out, after);
        }
        Kind::Inst(class, attrs) => {
            put_str(&mut out, class);
            put_pairs(&mut out, attrs);
        }
        Kind::Alias(_) => {}
        Kind::Value(v) => out.extend_from_slice(v),
    }
    let mut r = Vec::new();
    leb(&mut r, out.len() as u64);
    r.extend_from_slice(&out);
    r
}

fn compile(m: &Module) -> Result<Program, String> {
    mm_compile::compile(
        m,
        mm_compile::Options {
            last_expr_value: false,
        },
    )
    .map_err(|e| format!("prelude.py: {e:?}"))
}

/// Compiles `stmt` alone, as a module (prelude frames never appear in
/// tracebacks: drop what only they use).
fn compile_one(stmt: &Stmt) -> Result<Program, String> {
    let mut p = compile(&Module {
        body: vec![stmt.clone()],
    })?;
    strip(&mut p);
    Ok(p)
}

fn strip(p: &mut Program) {
    for code in p.codes.iter_mut().map(std::sync::Arc::make_mut) {
        code.positions.clear();
        code.reg_names.clear();
        code.first_line = 0;
    }
}

/// The `MAKE_FUNCTION` code indices of `c`, in order.
fn made(c: &Code) -> Vec<usize> {
    let mut out = Vec::new();
    let mut pc = 0;
    while pc < c.bytecode.len() {
        if opcode(&c.bytecode, pc) == op::MAKE_FUNCTION {
            out.push(operand(&c.bytecode, pc, 0));
        }
        pc += instr_len(opcode(&c.bytecode, pc));
    }
    out
}

/// The builtin type `type(<arg>)` names: `type(None)`, `type({}.keys())`,
/// `type({}.items())` or `type({}.values())`.
fn type_of_literal(call: &Expr) -> Option<&'static str> {
    let ExprKind::Call(func, args, kw) = &call.kind else {
        return None;
    };
    let [arg] = &args[..] else { return None };
    if name(func) != Some("type") || !kw.is_empty() {
        return None;
    }
    match &arg.kind {
        ExprKind::Const(Const::None) => Some("NoneType"),
        ExprKind::Call(method, margs, mkw) if margs.is_empty() && mkw.is_empty() => {
            let ExprKind::Attribute(obj, attr) = &method.kind else {
                return None;
            };
            if !matches!(&obj.kind, ExprKind::Dict(items) if items.is_empty()) {
                return None;
            }
            match attr.as_str() {
                "keys" => Some("dict_keys"),
                "items" => Some("dict_items"),
                "values" => Some("dict_values"),
                _ => None,
            }
        }
        _ => None,
    }
}

/// A scalar literal, encoded: a tag (0 `None`, 1 `False`, 2 `True`, 3 int
/// zigzag LEB128, 4 float 8 bytes little endian, 5 str, 9 a prelude or
/// builtin name's value, 10 the builtin type named), then its value.
fn scalar(expr: &Expr) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let int = |out: &mut Vec<u8>, v: i64| {
        out.push(3);
        leb(out, ((v << 1) ^ (v >> 63)).cast_unsigned());
    };
    let float = |out: &mut Vec<u8>, v: f64| {
        out.push(4);
        out.extend_from_slice(&v.to_le_bytes());
    };
    match &expr.kind {
        ExprKind::Const(Const::None) => out.push(0),
        ExprKind::Const(Const::Bool(v)) => out.push(1 + u8::from(*v)),
        ExprKind::Const(Const::Int(v)) => int(&mut out, *v),
        ExprKind::Const(Const::Float(v)) => float(&mut out, *v),
        ExprKind::Const(Const::Str(v)) => {
            out.push(5);
            put_str(&mut out, v);
        }
        ExprKind::Unary(UnaryOp::Neg, inner) => match &inner.kind {
            ExprKind::Const(Const::Int(v)) => int(&mut out, v.checked_neg()?),
            ExprKind::Const(Const::Float(v)) => float(&mut out, -v),
            _ => return None,
        },
        ExprKind::Name(n) => {
            out.push(9);
            put_str(&mut out, n);
        }
        ExprKind::Call(..) => {
            out.push(10);
            put_str(&mut out, type_of_literal(expr)?);
        }
        _ => return None,
    }
    Some(out)
}

/// A literal, encoded: a scalar, or a tuple (tag 6) or list (tag 8) of
/// scalars, or a dict of scalar keys and values (tag 7); containers are
/// their count, then their items.
fn literal(e: &Expr) -> Option<Vec<u8>> {
    let items = |tag: u8, items: &[Expr]| {
        let mut out = vec![tag];
        leb(&mut out, items.len() as u64);
        for i in items {
            out.extend(scalar(i)?);
        }
        Some(out)
    };
    match &e.kind {
        ExprKind::Tuple(i) => items(6, i),
        ExprKind::List(i) => items(8, i),
        ExprKind::Dict(pairs) => {
            let mut out = vec![7];
            leb(&mut out, pairs.len() as u64);
            for (k, v) in pairs {
                out.extend(scalar(k.as_ref()?)?);
                out.extend(scalar(v)?);
            }
            Some(out)
        }
        _ => scalar(e),
    }
}

/// The defaults and keyword defaults of `p`, if all are literals.
#[allow(clippy::type_complexity, reason = "build script")]
fn literal_defaults(p: &Params) -> Option<(Vec<Vec<u8>>, Vec<(String, Vec<u8>)>)> {
    let mut d = Vec::new();
    for a in p.posonly.iter().chain(&p.args) {
        if let Some(e) = &a.default {
            d.push(literal(e)?);
        }
    }
    let mut kw = Vec::new();
    for a in &p.kwonly {
        if let Some(e) = &a.default {
            kw.push((a.name.clone(), literal(e)?));
        }
    }
    Some((d, kw))
}

fn name(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Name(n) => Some(n),
        _ => None,
    }
}

/// A class whose body is undecorated or `@property` methods without
/// defaults or `__class__`, and `NAME = literal` assignments.
fn class_unit(stmt: &Stmt, module: Option<&str>) -> Result<Option<Unit>, String> {
    let StmtKind::ClassDef(c) = &stmt.kind else {
        return Ok(None);
    };
    let Some(module) = module else {
        return Ok(None);
    };
    let bases: Option<Vec<String>> = c.bases.iter().map(|b| name(b).map(String::from)).collect();
    let Some(bases) = bases else { return Ok(None) };
    if !c.decorators.is_empty() || !c.keywords.is_empty() {
        return Ok(None);
    }
    let prog = compile_one(stmt)?;
    let body = made(&prog.codes[prog.entry as usize]);
    let [body] = body[..] else { return Ok(None) };
    let mut methods = made(&prog.codes[body]).into_iter();
    let mut ns = Vec::new();
    for s in &c.body {
        match &s.kind {
            StmtKind::FunctionDef(f)
                if literal_defaults(&f.params)
                    .is_some_and(|(d, k)| d.is_empty() && k.is_empty()) =>
            {
                let Some(m) = methods.next() else {
                    return Ok(None);
                };
                if !prog.codes[m].freenames.is_empty() {
                    return Ok(None);
                }
                // An entry: tag (0 method, 1 property, 2 literal), then a
                // code index or the literal.
                let mut e = match &f.decorators[..] {
                    [] => vec![0],
                    [d] if name(d) == Some("property") => vec![1],
                    _ => return Ok(None),
                };
                leb(&mut e, m as u64);
                ns.push((f.name.clone(), e));
            }
            StmtKind::Assign(t, v) if t.len() == 1 && name(&t[0]).is_some() => {
                let Some(v) = literal(v) else { return Ok(None) };
                ns.push((
                    name(&t[0]).unwrap_or("").to_string(),
                    [&[2], &v[..]].concat(),
                ));
            }
            _ => return Ok(None),
        }
    }
    if methods.next().is_some() {
        return Ok(None);
    }
    Ok(Some(Unit {
        kind: Kind::Class(bases, module.to_string(), ns, Vec::new()),
        program: prog.encode(),
        line: stmt.pos.line,
    }))
}

/// The attributes `__init__` stores, in order, if its body is only
/// `self.<attr> = <parameter>` for each parameter in order.
fn plain_init(stmt: &Stmt) -> Option<Vec<String>> {
    let StmtKind::ClassDef(class) = &stmt.kind else {
        return None;
    };
    let init = class.body.iter().find_map(|st| match &st.kind {
        StmtKind::FunctionDef(f) if f.name == "__init__" => Some(f),
        _ => None,
    })?;
    let ps = &init.params;
    if !ps.posonly.is_empty() || ps.vararg.is_some() || !ps.kwonly.is_empty() || ps.kwarg.is_some()
    {
        return None;
    }
    let (me, params) = ps.args.split_first()?;
    if init.body.len() != params.len() {
        return None;
    }
    let mut attrs = Vec::new();
    for (st, param) in init.body.iter().zip(params) {
        let StmtKind::Assign(targets, value) = &st.kind else {
            return None;
        };
        let [target] = &targets[..] else { return None };
        let ExprKind::Attribute(obj, attr) = &target.kind else {
            return None;
        };
        if name(obj) != Some(me.name.as_str())
            || name(value) != Some(param.name.as_str())
            || param.default.is_some()
        {
            return None;
        }
        attrs.push(attr.clone());
    }
    Some(attrs)
}

fn main() -> Result<(), String> {
    println!("cargo:rerun-if-changed=src/prelude.py");
    let src = fs::read_to_string("src/prelude.py").map_err(|e| format!("src/prelude.py: {e}"))?;
    let module = mm_parse::parse(&src).map_err(|e| format!("prelude.py: {e:?}"))?;
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);
    let (module_name, units) = split(&module)?;
    write_units(&out_dir, &module_name, &units)?;
    // The VM-section codec (sections, `Cont` and host-value encodings,
    // snapshotted mode flags), whole.
    write_layout_hash(&out_dir, &["src/snapshot.rs"])
}

/// The prelude's final `__name__` and its units. Every top-level statement
/// is a unit or a `__name__ = '...'` assignment.
#[allow(
    clippy::too_many_lines,
    reason = "one pass over the top-level statements"
)]
fn split(module: &Module) -> Result<(String, BTreeMap<String, Unit>), String> {
    let mut units: BTreeMap<String, Unit> = BTreeMap::new();
    let mut inits: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // The prelude's `__name__` at each statement (classes record it).
    let mut module_name: Option<String> = None;
    for stmt in &module.body {
        let line = stmt.pos.line;
        if let StmtKind::Assign(targets, value) = &stmt.kind
            && let [target] = &targets[..]
            && let (Some("__name__"), ExprKind::Const(Const::Str(s))) = (name(target), &value.kind)
        {
            module_name = Some(s.clone());
            continue;
        }
        // The `asyncio` and `json` sections build only with their features.
        if module_name.as_deref().is_some_and(|m| !enabled(m)) {
            continue;
        }
        if let StmtKind::FunctionDef(f) = &stmt.kind
            && f.decorators.is_empty()
            && let Some((defaults, kwdefaults)) = literal_defaults(&f.params)
        {
            let prog = compile_one(stmt)?;
            let [func] = made(&prog.codes[prog.entry as usize])[..] else {
                return Err(format!("prelude.py: {}: one function expected", f.name));
            };
            let kind = Kind::Func(func, defaults, kwdefaults);
            units.insert(
                f.name.clone(),
                Unit {
                    kind,
                    program: prog.encode(),
                    line,
                },
            );
            continue;
        }
        if let Some(u) = class_unit(stmt, module_name.as_deref())? {
            if let StmtKind::ClassDef(c) = &stmt.kind {
                if let Some(a) = plain_init(stmt) {
                    inits.insert(c.name.clone(), a);
                }
                units.insert(c.name.clone(), u);
            }
            continue;
        }
        if let StmtKind::Assign(targets, value) = &stmt.kind
            && let [target] = &targets[..]
        {
            // `NAME = OTHER`, another unit.
            if let (Some(n), Some(other)) = (name(target), name(value))
                && units
                    .get(other)
                    .is_some_and(|u| !matches!(u.kind, Kind::Alias(_)))
                && !units.contains_key(n)
            {
                let kind = Kind::Alias(other.to_string());
                units.insert(
                    n.to_string(),
                    Unit {
                        kind,
                        program: Vec::new(),
                        line,
                    },
                );
                continue;
            }
            // `C.attr = literal` after class unit `C`'s body.
            if let ExprKind::Attribute(o, attr) = &target.kind
                && let Some(c) = name(o)
                && let Some(v) = literal(value)
                && let Some(Unit {
                    kind: Kind::Class(_, _, _, after),
                    ..
                }) = units.get_mut(c)
            {
                after.push((attr.clone(), v));
                continue;
            }
            // `NAME = C(literals)`, for class unit `C` whose `__init__` only
            // stores its parameters.
            if let Some(n) = name(target)
                && let ExprKind::Call(f, args, kw) = &value.kind
                && kw.is_empty()
                && let Some(c) = name(f)
                && let Some(attrs) = inits.get(c)
                && attrs.len() == args.len()
                && !units.contains_key(n)
            {
                let vals: Option<Vec<Vec<u8>>> = args.iter().map(literal).collect();
                if let Some(vals) = vals {
                    let a = attrs.iter().cloned().zip(vals).collect();
                    let kind = Kind::Inst(c.to_string(), a);
                    units.insert(
                        n.to_string(),
                        Unit {
                            kind,
                            program: Vec::new(),
                            line,
                        },
                    );
                    continue;
                }
            }
            // `NAME = literal`.
            if let Some(n) = name(target)
                && let Some(v) = literal(value)
                && !units.contains_key(n)
            {
                let kind = Kind::Value(v);
                units.insert(
                    n.to_string(),
                    Unit {
                        kind,
                        program: Vec::new(),
                        line,
                    },
                );
                continue;
            }
        }
        return Err(format!(
            "prelude.py:{line}: a top-level statement must be a def with literal defaults, a class \
             of methods and literals, NAME = literal, NAME = OTHER, NAME = Class(literals), \
             Class.attr = literal or __name__ = '...'"
        ));
    }
    let name = module_name.ok_or("prelude.py: no __name__")?;
    Ok((name, units))
}

/// Whether the prelude section named `module` (its `__name__`) is in this
/// build: the `asyncio` and `json` sections need their features.
fn enabled(module: &str) -> bool {
    let feature = match module.split('.').next() {
        Some("asyncio") => "CARGO_FEATURE_ASYNCIO",
        Some("json") => "CARGO_FEATURE_JSON",
        _ => return true,
    };
    env::var_os(feature).is_some()
}

/// 64-bit FNV-1a over `parts`, each preceded by its length (u64,
/// little-endian).
fn fnv1a(parts: &[&[u8]]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for &b in (part.len() as u64).to_le_bytes().iter().chain(*part) {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

/// Writes `prelude_chunks.bin` and `prelude_units.rs`. Unit records (see
/// [`record`]) go into chunks of about [`CHUNK`] bytes in source order,
/// packed against one shared Huffman header. The Rust side gets the chunk
/// bounds, the sorted unit names and one row per unit.
fn write_units(
    out_dir: &Path,
    module_name: &str,
    units: &BTreeMap<String, Unit>,
) -> Result<(), String> {
    let mut order: Vec<&String> = units
        .keys()
        .filter(|n| !matches!(units[*n].kind, Kind::Alias(_)))
        .collect();
    order.sort_by_key(|n| units[*n].line);
    let (programs, owned) = unit_programs(units)?;
    let shared: Vec<&str> = owned.iter().map(String::as_str).collect();
    let (text, ends) = mm_compile::SharedStrings::build(&shared)
        .ok_or_else(|| String::from("prelude shared strings: more than 64 KiB"))?;
    // Each chunk: the nine program sections of its units, one after
    // another, and the unit records (`record`: each unit's section lengths,
    // then what its kind needs).
    let mut chunks: Vec<[Vec<u8>; PARTS]> = vec![Default::default()];
    let mut size = 0;
    let mut place: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    for n in order {
        let u = &units[n];
        let sections = programs
            .get(n.as_str())
            .map(|p| p.encode_sections(&shared))
            .unwrap_or_default();
        let r = record(u, &sections);
        let bytes = sections.iter().map(Vec::len).sum::<usize>() + r.len();
        if size > 0 && size + bytes > CHUNK {
            chunks.push(Default::default());
            size = 0;
        }
        size += bytes;
        let c = chunks.len() - 1;
        let records = &mut chunks[c][PARTS - 1];
        place.insert(n, (c, records.len(), r.len()));
        records.extend_from_slice(&r);
        for (k, part) in sections.iter().enumerate() {
            chunks[c][k].extend_from_slice(part);
        }
    }
    let (blob, header_ends, bounds) = pack_chunks(&chunks)?;
    let header_len = header_ends[PARTS - 1];
    check_sections(&programs, &place, &chunks, &text, &ends)?;
    // The build id covers the whole prelude, wherever its chunks are
    // (`host-bundle` leaves them to the host).
    let hash = fnv1a(&[&blob, text.as_bytes(), &ends]);
    let external = env::var_os("CARGO_FEATURE_HOST_BUNDLE").is_some();
    let path = out_dir.join("prelude_chunks.bin");
    let linked = if external {
        &blob[..header_len]
    } else {
        &blob[..]
    };
    fs::write(&path, linked).map_err(|e| format!("{}: {e}", path.display()))?;
    if external {
        let path = out_dir.join("prelude_bundle.bin");
        fs::write(&path, &blob[header_len..]).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let path = out_dir.join("prelude_strings.txt");
    fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    let path = out_dir.join("prelude_string_ends.bin");
    fs::write(&path, &ends).map_err(|e| format!("{}: {e}", path.display()))?;

    let fit = |v: usize| u16::try_from(v).map_err(|_| String::from("prelude units: too large"));
    let index: BTreeMap<&str, usize> = units
        .keys()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    let (mut names, mut table) = (String::new(), String::new());
    for (n, u) in units {
        names.push_str(n);
        let (chunk, at, len) = place.get(n.as_str()).copied().unwrap_or((0, 0, 0));
        let (chunk, kind) = match &u.kind {
            Kind::Func(..) => (chunk, 0),
            Kind::Class(..) => (chunk, 1),
            Kind::Inst(..) => (chunk, 2),
            Kind::Alias(other) => (index[other.as_str()], 3),
            Kind::Value(_) => (chunk, 4),
        };
        let _ = writeln!(
            table,
            "    [{}, {}, {}, {}, {kind}],",
            fit(names.len())?,
            fit(chunk)?,
            fit(at)?,
            fit(len)?
        );
    }
    let mut rs = String::new();
    let _ = writeln!(
        rs,
        "/// The parts' shared Huffman headers, then the packed chunks: each the\n/// lengths of its first `PARTS - 1` parts, then its parts. With `host-bundle`\n/// it ends after the headers, the chunks are the host's, and `CHUNK_END`\n/// counts as if they followed.\nstatic CHUNKS: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/prelude_chunks.bin\"));\n/// FNV-1a of the packed prelude and its strings, for the snapshot build id\n/// (computed at build time, the same with and without `host-bundle`).\nconst PRELUDE_HASH: u64 = 0x{:04x}_{:04x}_{:04x}_{:04x};\n/// Where each part's header ends in `CHUNKS` (the first starts at 0).\nconst HEADER_END: [usize; PARTS] = {header_ends:?};\n/// The headers' length.\nconst HEADER: usize = {header_len};\n/// Where each chunk ends in `CHUNKS` (the first starts after the headers).\nstatic CHUNK_END: [u32; {}] = {bounds:?};\n/// The strings three or more unit programs use, one after another.\nstatic TEXT: &str = include_str!(concat!(env!(\"OUT_DIR\"), \"/prelude_strings.txt\"));\n/// Where each string ends in `TEXT`, as little-endian `u16`s.\nstatic ENDS: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/prelude_string_ends.bin\"));\n/// The unit names, sorted, concatenated.\nstatic NAMES: &str = {names:?};\n/// The prelude's `__name__`.\nconst PRELUDE_NAME: &str = {module_name:?};",
        hash >> 48,
        (hash >> 32) & 0xffff,
        (hash >> 16) & 0xffff,
        hash & 0xffff,
        bounds.len()
    );
    let _ = writeln!(
        rs,
        "/// By unit: the end of its name in `NAMES`; its chunk, and the offset\n/// and length of its record in the chunk's unpacked records; its kind (`FUNC`,\n/// `CLASS`, `INST`, `ALIAS`, `VALUE`). An alias's chunk is its target's
/// index.\nstatic UNITS: [[u16; 5]; {}] = [\n{table}];",
        units.len()
    );
    let path = out_dir.join("prelude_units.rs");
    fs::write(&path, rs).map_err(|e| format!("{}: {e}", path.display()))
}

/// The chunks' parts, each packed against one set of codes for every
/// chunk: the headers, then each chunk (the lengths of its first parts,
/// then its parts); where each header and each chunk ends.
/// The packed blob, the header ends and the chunk ends.
type Packed = (Vec<u8>, Vec<usize>, Vec<usize>);

fn pack_chunks(chunks: &[[Vec<u8>; PARTS]]) -> Result<Packed, String> {
    let mut blob = Vec::new();
    let mut header_ends = Vec::new();
    let mut packed: Vec<Vec<Vec<u8>>> = vec![Vec::new(); chunks.len()];
    for k in 0..PARTS {
        let parts: Vec<&[u8]> = chunks.iter().map(|c| c[k].as_slice()).collect();
        let (header, out) = mm_compile::pack::pack_shared(&parts);
        for (i, (p, c)) in out.iter().zip(chunks).enumerate() {
            let mut budget = u64::MAX;
            if mm_compile::pack::unpack_shared_bounded(&header, p, &mut budget).as_deref()
                != Ok(&c[k][..])
            {
                return Err(format!("prelude chunk {i}: part {k} does not round-trip"));
            }
        }
        blob.extend_from_slice(&header);
        header_ends.push(blob.len());
        for (m, part) in packed.iter_mut().zip(out) {
            m.push(part);
        }
    }
    let mut bounds = Vec::new();
    for parts in &packed {
        for part in &parts[..PARTS - 1] {
            leb(&mut blob, part.len() as u64);
        }
        for part in parts {
            blob.extend_from_slice(part);
        }
        bounds.push(blob.len());
    }
    Ok((blob, header_ends, bounds))
}

/// The units' programs by name, and the strings three or more of them use
/// (stored once, raw), most used first.
fn unit_programs(
    units: &BTreeMap<String, Unit>,
) -> Result<(BTreeMap<&str, Program>, Vec<String>), String> {
    let programs: BTreeMap<&str, Program> = units
        .iter()
        .filter(|(_, u)| !u.program.is_empty())
        .map(|(n, u)| {
            Program::decode(&u.program)
                .map(|p| (n.as_str(), p))
                .ok_or_else(|| format!("prelude unit {n}: does not decode"))
        })
        .collect::<Result<_, _>>()?;
    let mut users: BTreeMap<String, usize> = BTreeMap::new();
    for p in programs.values() {
        for s in p.strings() {
            *users.entry(s).or_default() += 1;
        }
    }
    let mut shared: Vec<(String, usize)> = users.into_iter().filter(|&(_, n)| n >= 3).collect();
    shared.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok((programs, shared.into_iter().map(|(s, _)| s).collect()))
}

/// Every unit's program decodes from its sections in its chunk.
fn check_sections(
    programs: &BTreeMap<&str, Program>,
    place: &BTreeMap<&str, (usize, usize, usize)>,
    chunks: &[[Vec<u8>; PARTS]],
    text: &str,
    ends: &[u8],
) -> Result<(), String> {
    let strings = mm_compile::SharedStrings::new(text, ends);
    for (n, p) in programs {
        let (c, at, _) = place[n];
        let chunk = &chunks[c];
        let mut offsets = [0usize; mm_compile::section::COUNT];
        let mut r = &chunk[PARTS - 1][..];
        while r.len() > chunk[PARTS - 1].len() - at {
            let len = read_leb(&mut r)?;
            let (rec, rest) = r.split_at(len);
            let mut rec = rec;
            for o in &mut offsets {
                *o += read_leb(&mut rec)?;
            }
            r = rest;
        }
        let _ = read_leb(&mut r)?;
        let mut sections: [&[u8]; mm_compile::section::COUNT] = [&[]; mm_compile::section::COUNT];
        for (k, s) in sections.iter_mut().enumerate() {
            let len = read_leb(&mut r)?;
            *s = &chunk[k][offsets[k]..offsets[k] + len];
        }
        let decoded = Program::decode_sections(&sections, strings, &mut { u64::MAX })
            .map_err(|e| format!("prelude unit {n}: sections do not decode: {e:?}"))?;
        if decoded.codes != p.codes || decoded.entry != p.entry {
            return Err(format!("prelude unit {n}: sections do not round-trip"));
        }
    }
    Ok(())
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
