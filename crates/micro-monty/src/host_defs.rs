//! The host surface: [`HostModule`]s of host functions, host classes and
//! constants ([`Session::define`]), and their `.pyi` stubs ([Host functions
//! and host modules](crate#host-functions-and-host-modules)).
//!
//! Typed functions and classes are Python the facade generates, one
//! program per module, and the engine runs once, between cells, in the
//! prelude's globals ([`mm_engine::Vm::run_host_defs`]): hidden from
//! tracebacks, kept across `reset`, saved in snapshots. A typed function is
//! an `_mm.signature` function, so arguments bind (and fail) as CPython's
//! do, and then its parameters are checked against the annotations the
//! boundary can see before the plain host function is called with them in
//! parameter order. A host class is a tuple subclass with named fields,
//! which crosses the boundary as [`HostValue::Object`]. The engine keeps
//! each module's definition as a plain value, for [`Session::stubs`] and
//! [`Session::host_modules`].

use crate::{PyException, Session};
use mm_engine::text::debug_quoted;
use mm_engine::{HostErrorKind, HostProgram, HostValue};
use std::fmt::Write as _;
use std::sync::Arc;

/// How a parameter of a host function is passed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParamKind {
    /// By position or by keyword (the default).
    #[default]
    PositionalOrKeyword,
    /// By keyword only (after `*`).
    KeywordOnly,
    /// `*args`: only in the untyped form `(*args, **kwargs)`
    /// ([`HostSignature::any`]).
    VarPositional,
    /// `**kwargs`: only in the untyped form `(*args, **kwargs)`.
    VarKeyword,
}

/// A parameter of a [`HostSignature`]. Build it with
/// `..HostParam::default()` for the fields you leave out, so later fields
/// do not break your code.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostParam {
    /// The name: an ASCII identifier not starting with `_host`.
    pub name: String,
    /// A type annotation as Python text (`str`, `int | None`,
    /// `list[str]`, a host class name), or empty for `Any`. It is checked
    /// at the call when every `|` part is one of `None`, `str`, `bytes`,
    /// `bool`, `int`, `float` (also accepts `int`), `list`/`tuple`
    /// (either), `dict` or a defined host class; otherwise it only appears
    /// in the stubs.
    pub annotation: String,
    /// The default value; positional parameters with defaults come last.
    pub default: Option<HostValue>,
    /// How it is passed.
    pub kind: ParamKind,
}

/// A host function or host class method.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostSignature {
    /// The bare name (the module comes from [`HostModule::name`]). Untyped
    /// functions of the builtins module may also use the dotted names of
    /// the OS hooks (`os.getenv`, `Path.read_text`; see
    /// [`Session::define`]).
    pub name: String,
    /// The parameters, keyword-only ones last (a method's `self` is
    /// implicit). Exactly `*args, **kwargs` makes the function untyped
    /// ([`HostSignature::any`]).
    pub params: Vec<HostParam>,
    /// The return annotation for the stubs (not checked); empty for `Any`.
    pub returns: String,
    /// The docstring: `__doc__`, `help()` and the stubs.
    pub doc: String,
    /// Whether a typed function also accepts one positional `dict` as its
    /// keyword arguments: `f({'path': 'a', 'limit': 3})` binds as
    /// `f(path='a', limit=3)`, then checks the annotations as usual.
    /// Off by default: it is ambiguous for a function whose only
    /// parameter is itself a dict. Untyped functions ignore it (they
    /// report a dict as given).
    pub dict_as_kwargs: bool,
}

impl HostSignature {
    /// An untyped host function, `name(*args: Any, **kwargs: Any) -> Any`:
    /// its call reports the arguments as given, keyword arguments in
    /// `kwargs`.
    #[must_use]
    pub fn any(name: &str) -> Self {
        let param = |name: &str, kind| HostParam {
            name: name.to_owned(),
            annotation: String::new(),
            default: None,
            kind,
        };
        HostSignature {
            name: name.to_owned(),
            params: vec![
                param("args", ParamKind::VarPositional),
                param("kwargs", ParamKind::VarKeyword),
            ],
            returns: String::new(),
            doc: String::new(),
            dict_as_kwargs: false,
        }
    }

    /// Whether this is the untyped form `(*args, **kwargs)`.
    #[must_use]
    pub fn is_untyped(&self) -> bool {
        matches!(
            self.params.as_slice(),
            [a, k] if a.kind == ParamKind::VarPositional && k.kind == ParamKind::VarKeyword
        )
    }
}

/// A host class: plain data with named fields whose methods are host
/// functions ([Host classes](crate#host-classes)).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostClass {
    /// The bare class name.
    pub name: String,
    /// Field names and annotations (for the stubs), in order.
    pub fields: Vec<(String, String)>,
    /// Methods, all typed: a call reports `<class>.<method>` (with the
    /// module prefix) with the instance (as [`HostValue::Object`]) as the
    /// first argument, then the parameters.
    pub methods: Vec<HostSignature>,
    /// The docstring.
    pub doc: String,
}

/// A host exception class ([Host errors](crate#host-errors)): a subclass of
/// `base` that cells can catch by name (`except ToolError as e:`). A
/// [`HostError`](crate::HostError) whose `class` names it raises an
/// instance whose `tool` attribute is the call name and `data` the
/// error's payload; both are `None` on instances built in Python.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostException {
    /// The bare class name.
    pub name: String,
    /// The built-in exception class it derives from (default
    /// `Exception`).
    pub base: HostErrorKind,
    /// The docstring.
    pub doc: String,
}

/// Everything the host defines in one module, given to [`Session::define`]
/// as a whole. Plain data: build it once and define it in every session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostModule {
    /// `""` for the builtins (visible in every cell, shadowable by
    /// globals), else the module name for `import`; a dotted name is a
    /// submodule, also an attribute of its parent.
    pub name: String,
    /// Host functions: typed, or untyped ([`HostSignature::any`]).
    pub functions: Vec<HostSignature>,
    /// Host classes.
    pub classes: Vec<HostClass>,
    /// Constant attributes.
    pub constants: Vec<(String, HostValue)>,
    /// Exception classes for [`HostError::class`](crate::HostError::class).
    pub exceptions: Vec<HostException>,
}

/// Why [`Session::define`] refused a module. Nothing changes then: the
/// module's previous definition, if any, stays.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DefineError {
    /// The definition is invalid, the host's mistake: a bad, repeated or
    /// reserved name (a module named like one of micro-monty's own), a bad
    /// parameter list, a value nested too deep. The text says which.
    Invalid(String),
    /// A cell is stopped: finish or cancel it first.
    Busy,
    /// Python raised while defining it: `MemoryError` when the definition
    /// does not fit the memory limit.
    Raised(PyException),
    /// An engine panic poisoned the session (see the crate docs); the text
    /// is the panic's report.
    Poisoned(String),
}

impl std::fmt::Display for DefineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DefineError::Invalid(message) | DefineError::Poisoned(message) => f.write_str(message),
            DefineError::Busy => f.write_str(
                "cannot define a host module while a cell is stopped; finish or cancel it first",
            ),
            DefineError::Raised(e) => write!(f, "{}: {}", e.type_name, e.message),
        }
    }
}

impl std::error::Error for DefineError {}

/// Deepest nesting of a constant or default value (the engine's ceiling on
/// host values).
const MAX_VALUE_DEPTH: usize = 1000;

/// Whether `v` nests containers deeper than [`MAX_VALUE_DEPTH`], checked
/// on an explicit stack before anything clones or converts it.
fn too_deep(v: &HostValue) -> bool {
    let mut stack = vec![(v, 0usize)];
    while let Some((v, depth)) = stack.pop() {
        let items: Vec<&HostValue> = match v {
            HostValue::List(items) | HostValue::Tuple(items) => items.iter().collect(),
            HostValue::Dict(fields)
            | HostValue::Record { fields, .. }
            | HostValue::Object { fields, .. } => fields.iter().map(|(_, v)| v).collect(),
            _ => continue,
        };
        if depth >= MAX_VALUE_DEPTH {
            return true;
        }
        stack.extend(items.into_iter().map(|v| (v, depth + 1)));
    }
    false
}

/// Rejects names that would not be safe to place in generated Python.
fn check_ident(what: &str, name: &str) -> Result<(), DefineError> {
    let mut chars = name.chars();
    let ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with("_host");
    if ok {
        Ok(())
    } else {
        Err(value_error(&format!(
            "invalid {what} name {}",
            debug_quoted(name)
        )))
    }
}

fn value_error(message: &str) -> DefineError {
    DefineError::Invalid(message.to_owned())
}

/// Splits `a | b[c | d]` at top-level `|`.
fn union_parts(annotation: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (i, c) in annotation.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            '|' if depth == 0 => {
                parts.push(annotation[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(annotation[start..].trim());
    parts
}

/// A Python condition that holds when `var` matches `annotation`, or
/// `None` when the annotation is not checked. `class` maps a host class
/// name to a Python expression for the class.
fn check_expr(
    annotation: &str,
    var: &str,
    class: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    if annotation.trim().is_empty() {
        return None;
    }
    let mut alternatives = Vec::new();
    for part in union_parts(annotation) {
        let base = part.split('[').next().unwrap_or(part).trim();
        alternatives.push(match base {
            "None" => format!("{var} is None"),
            "str" | "bytes" | "bool" | "int" | "dict" => format!("isinstance({var}, {base})"),
            "float" => format!("isinstance({var}, (int, float))"),
            "list" | "tuple" => format!("isinstance({var}, (list, tuple))"),
            other => format!("isinstance({var}, {})", class(other)?),
        });
    }
    Some(alternatives.join(" or "))
}

/// Python source and binds for one typed callable: defines
/// `_host_impl{i}` (the checks, then `raw(*params)`) inside the enclosing
/// generated function, and returns the expression building the
/// `_mm.signature` function. Messages, defaults and docs are bound values,
/// never source text.
struct Callable<'a> {
    sig: &'a HostSignature,
    /// What error messages call it (`read`, `Entry.read`).
    shown: String,
    /// The receiver parameter of a method.
    receiver: Option<&'a str>,
}

impl Callable<'_> {
    fn check(&self) -> Result<(), DefineError> {
        let mut seen_default = false;
        let mut seen_kwonly = false;
        for p in &self.sig.params {
            check_ident("parameter", &p.name)?;
            if self.receiver == Some(p.name.as_str()) {
                return Err(value_error(&format!(
                    "parameter {} is the receiver",
                    debug_quoted(&p.name)
                )));
            }
            match p.kind {
                ParamKind::VarPositional | ParamKind::VarKeyword => {
                    return Err(value_error(&format!(
                        "{}: *args and **kwargs are only supported as the untyped form \
                         (*args, **kwargs)",
                        self.shown
                    )));
                }
                ParamKind::KeywordOnly => seen_kwonly = true,
                ParamKind::PositionalOrKeyword if seen_kwonly => {
                    return Err(value_error("keyword-only parameters must come last"));
                }
                ParamKind::PositionalOrKeyword => {
                    if p.default.is_some() {
                        seen_default = true;
                    } else if seen_default {
                        return Err(value_error(
                            "a parameter without a default follows one with a default",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Appends `_host_impl{i}` to `src` at `indent`; pushes its messages.
    fn write_impl(
        &self,
        src: &mut String,
        indent: &str,
        i: usize,
        raw: &str,
        messages: &mut Vec<HostValue>,
        class: &dyn Fn(&str) -> Option<String>,
    ) {
        let names: Vec<&str> = self
            .receiver
            .into_iter()
            .chain(self.sig.params.iter().map(|p| p.name.as_str()))
            .collect();
        let _ = writeln!(src, "{indent}def _host_impl{i}({}):", names.join(", "));
        for p in &self.sig.params {
            if let Some(cond) = check_expr(&p.annotation, &p.name, class) {
                let _ = writeln!(src, "{indent}    if not ({cond}):");
                let _ = writeln!(
                    src,
                    "{indent}        raise TypeError(_host_msgs[{}] + type({}).__name__)",
                    messages.len(),
                    p.name
                );
                messages.push(HostValue::Str(format!(
                    "{}() argument '{}' must be {}, not ",
                    self.shown, p.name, p.annotation
                )));
            }
        }
        let _ = writeln!(src, "{indent}    return {raw}({})", names.join(", "));
    }

    /// The `_mm.signature(...)` call for `_host_impl{i}`; defaults and the
    /// docstring are `_host_data[i]`: `[defaults, kwdefaults, doc]`.
    fn signature_call(&self, i: usize, qualname: &str) -> String {
        let quote = |names: Vec<&str>| {
            let mut t = names
                .iter()
                .map(|n| format!("'{n}'"))
                .collect::<Vec<_>>()
                .join(", ");
            if names.len() == 1 {
                t.push(',');
            }
            format!("({t})")
        };
        let positional: Vec<&str> = self
            .receiver
            .into_iter()
            .chain(
                self.sig
                    .params
                    .iter()
                    .filter(|p| p.kind == ParamKind::PositionalOrKeyword)
                    .map(|p| p.name.as_str()),
            )
            .collect();
        let kwonly: Vec<&str> = self
            .sig
            .params
            .iter()
            .filter(|p| p.kind == ParamKind::KeywordOnly)
            .map(|p| p.name.as_str())
            .collect();
        let f = format!(
            "_host_doc(_mm.signature('{}', '{qualname}', {}, {}, tuple(_host_data[{i}][0]) or None, \
             _host_data[{i}][1] or None, _host_impl{i}), _host_data[{i}][2], _host_module)",
            self.sig.name,
            quote(positional),
            quote(kwonly)
        );
        if self.sig.dict_as_kwargs {
            format!("_host_kw({f}, {})", usize::from(self.receiver.is_some()))
        } else {
            f
        }
    }

    /// `[defaults, kwdefaults, doc]` for [`Callable::signature_call`].
    fn data(&self) -> HostValue {
        let defaults = self
            .sig
            .params
            .iter()
            .filter(|p| p.kind == ParamKind::PositionalOrKeyword)
            .filter_map(|p| p.default.clone())
            .collect();
        let kwdefaults = self
            .sig
            .params
            .iter()
            .filter(|p| p.kind == ParamKind::KeywordOnly)
            .filter_map(|p| Some((p.name.clone(), p.default.clone()?)))
            .collect();
        HostValue::List(vec![
            HostValue::List(defaults),
            HostValue::Dict(kwdefaults),
            HostValue::Str(self.sig.doc.clone()),
        ])
    }

    /// The `def` line of the stub, at `indent`.
    fn stub(&self, indent: &str) -> String {
        let mut params: Vec<String> = self.receiver.iter().map(|r| (*r).to_owned()).collect();
        let mut star = false;
        for p in &self.sig.params {
            if p.kind == ParamKind::KeywordOnly && !star {
                params.push("*".to_owned());
                star = true;
            }
            let mut text = match p.kind {
                ParamKind::VarPositional => format!("*{}", p.name),
                ParamKind::VarKeyword => format!("**{}", p.name),
                _ => p.name.clone(),
            };
            let annotation = if p.annotation.is_empty() && text.starts_with('*') {
                "Any"
            } else {
                &p.annotation
            };
            if !annotation.is_empty() {
                let _ = write!(text, ": {annotation}");
            }
            if p.default.is_some() {
                text.push_str(if annotation.is_empty() {
                    "=..."
                } else {
                    " = ..."
                });
            }
            params.push(text);
        }
        let returns = if self.sig.returns.is_empty() {
            "Any"
        } else {
            &self.sig.returns
        };
        let mut out = format!(
            "{indent}def {}({}) -> {returns}:",
            self.sig.name,
            params.join(", ")
        );
        if self.sig.doc.is_empty() {
            out.push_str(" ...\n");
        } else {
            let _ = write!(out, "\n{indent}    {}\n", docstring(&self.sig.doc));
        }
        out
    }
}

/// `doc` as a Python string literal for a stub.
fn docstring(doc: &str) -> String {
    let escaped = doc.replace('\\', "\\\\").replace("\"\"\"", "\\\"\\\"\\\"");
    format!("\"\"\"{escaped}\"\"\"")
}

/// The helpers every generated program starts with.
const PRELUDE: &str = concat!(
    "import _mm\n",
    "def _host_doc(f, doc, module):\n",
    "    f.__doc__ = doc or None\n",
    "    f.__module__ = module\n",
    "    return f\n",
    // `dict_as_kwargs`: one dict after the receiver binds as keywords.
    "def _host_kw(f, skip):\n",
    "    def call(*args, **kwargs):\n",
    "        if len(args) == skip + 1 and not kwargs and isinstance(args[skip], dict):\n",
    "            return f(*args[:skip], **args[skip])\n",
    "        return f(*args, **kwargs)\n",
    "    call.__name__ = f.__name__\n",
    "    call.__qualname__ = f.__qualname__\n",
    "    call.__doc__ = f.__doc__\n",
    "    call.__module__ = f.__module__\n",
    "    return call\n",
);

/// A host function [`Session::define`] defined: its call name (what
/// [`crate::Progress::HostCall`] reports as `name`) and its id (`function`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HostFunctionRef {
    /// `function`, `module.function` or `module.Class.method`.
    pub name: String,
    /// Stable per name for the session's lifetime, snapshots and templates
    /// included.
    pub id: u32,
}

impl HostFunctionRef {
    /// A reference, for hosts that build [`HostCallRecord`](crate::HostCallRecord)s themselves
    /// (tests, replay).
    pub fn new(name: impl Into<String>, id: u32) -> Self {
        HostFunctionRef {
            name: name.into(),
            id,
        }
    }
}

impl HostModule {
    /// Module `name` (`""` for the builtins) of untyped functions.
    #[must_use]
    pub fn untyped(name: &str, functions: &[&str]) -> Self {
        HostModule {
            name: name.to_owned(),
            functions: functions.iter().map(|f| HostSignature::any(f)).collect(),
            ..HostModule::default()
        }
    }
}

/// The shared code of module `name`'s generated `src`: the program the
/// Engine's cache holds under its key, else compiled now and offered to
/// the cache, with the hash `definition` gives of the definition and the
/// recipe ([`HostProgram::definition_hash`]).
fn host_program(
    cache: Option<&Arc<dyn mm_engine::ModuleCache>>,
    name: &str,
    src: &str,
    recipe: Vec<String>,
    definition: &dyn Fn(&[String]) -> u64,
) -> Result<Arc<HostProgram>, DefineError> {
    let key = crate::image::host_key(src);
    if let Some(p) = cache.and_then(|c| c.host_program(&key)) {
        return Ok(p);
    }
    // Host definitions are the host's own input: no source limit.
    let code = crate::Engine::compile_source(src, "<host>").map_err(|e| {
        DefineError::Invalid(format!(
            "module {} does not compile: {}: {}",
            debug_quoted(name),
            e.type_name,
            e.message
        ))
    })?;
    let mut program = code.program;
    for c in &mut program.codes {
        if let Some(c) = Arc::get_mut(c) {
            c.bytecode.shrink_to_fit();
            c.consts.shrink_to_fit();
            c.positions.shrink_to_fit();
        }
    }
    let p = Arc::new(HostProgram {
        key,
        code: HostProgram::code_hash(&program),
        program,
        definition: definition(&recipe),
        recipe,
    });
    if let Some(c) = cache {
        c.offer_host_program(&p);
    }
    Ok(p)
}

/// The code a restored image's host module `name` (`spec`, `recipe`) ran,
/// for an Engine whose cache does not hold it for this definition:
/// generated again, then taken from the cache by key or compiled (and
/// cached). [`mm_engine::Vm::read_snapshot`] refuses it unless its key is
/// the image's.
pub(crate) fn rebuild_host_program(
    cache: Option<&Arc<dyn mm_engine::ModuleCache>>,
    name: &str,
    spec: &HostValue,
    recipe: &[String],
) -> Option<Arc<HostProgram>> {
    let module = decode_module(spec).filter(|m| m.name == name)?;
    let g = generate(&module, &|| recipe.to_vec()).ok()?;
    host_program(cache, name, &g.src, g.recipe, &|recipe| {
        HostProgram::definition_hash(name, spec, recipe)
    })
    .ok()
}

/// A hash (xxh64) of [`probe_code`]: the spec an image stores for a
/// definition that uses every kind of parameter, defaults, annotations,
/// `dict_as_kwargs`, an untyped function, a class with a method and an
/// exception, the source [`Session::define`] generates from that spec, and
/// the encoded program it compiles to. Part of the snapshot build id
/// ([`crate::build_id`]): a restored image's host modules are generated and
/// compiled again from their specs unless it holds their code, and their
/// frames resume in that code, so a build whose spec codec, generator or
/// compiler gives other code must not restore the image. (A restore also
/// checks each rebuilt program's key and code hash, `HostProgram::code`.)
///
/// Recorded, so that `build_id()` compiles nothing at run time: when the
/// spec codec, the generator or the compiler change the probe's code, the
/// test `the_codegen_probe_matches_its_recorded_hash` fails and prints the
/// new value to set here, which changes the build id.
pub(crate) const CODEGEN_PROBE: u64 = 0x9960_5814_997f_4403;

/// What [`CODEGEN_PROBE`] hashes, or why the probe definition does not
/// generate or compile (a bug).
#[cfg(test)]
fn probe_code() -> Result<Vec<u8>, String> {
    let p = |name: &str, annotation: &str, default: Option<HostValue>, kind| HostParam {
        name: name.to_owned(),
        annotation: annotation.to_owned(),
        default,
        kind,
    };
    let f = HostSignature {
        name: "fetch".to_owned(),
        params: vec![
            p("url", "str", None, ParamKind::PositionalOrKeyword),
            p(
                "limit",
                "int | None",
                Some(HostValue::None),
                ParamKind::PositionalOrKeyword,
            ),
            p("tags", "list[str]", None, ParamKind::KeywordOnly),
            p(
                "at",
                "Point | None",
                Some(HostValue::None),
                ParamKind::KeywordOnly,
            ),
        ],
        returns: "dict".to_owned(),
        doc: "Fetch.".to_owned(),
        dict_as_kwargs: true,
    };
    let method = HostSignature {
        name: "scaled".to_owned(),
        params: vec![p(
            "k",
            "float",
            Some(HostValue::Float(1.5)),
            ParamKind::PositionalOrKeyword,
        )],
        ..HostSignature::default()
    };
    let module = HostModule {
        name: "probe".to_owned(),
        functions: vec![f, HostSignature::any("log")],
        classes: vec![HostClass {
            name: "Point".to_owned(),
            fields: vec![
                ("x".to_owned(), "int".to_owned()),
                ("y".to_owned(), String::new()),
            ],
            methods: vec![method],
            doc: String::new(),
        }],
        constants: vec![("LIMIT".to_owned(), HostValue::Int(10))],
        exceptions: vec![HostException {
            name: "ToolError".to_owned(),
            base: HostErrorKind::ValueError,
            doc: String::new(),
        }],
    };
    // As a restore rebuilds it: from the spec an image stores.
    let spec = encode_module(&module);
    let module = decode_module(&spec).ok_or("the spec does not decode")?;
    let g = generate(&module, &Vec::new).map_err(|e| format!("{e:?}"))?;
    let code = crate::Engine::compile_source(&g.src, "<host>").map_err(|e| e.message)?;
    let mut out = Vec::new();
    mm_engine::put_host_value(&mut out, &spec);
    out.extend_from_slice(g.src.as_bytes());
    out.extend_from_slice(&code.program.encode());
    Ok(out)
}

/// `module.attr`, or `attr` for the builtins module.
fn key(module: &str, attr: &str) -> String {
    if module.is_empty() {
        attr.to_owned()
    } else {
        format!("{module}.{attr}")
    }
}

impl Session {
    /// Defines host module `module` ([Host functions and host
    /// modules](crate#host-functions-and-host-modules)), replacing whatever was defined under its name before,
    /// whole: functions, classes and constants not in `module` are gone.
    /// `name == ""` defines builtins, visible in every cell; other names
    /// are modules for `import` (calls report `module.function`).
    /// Definitions survive [`Session::reset`], snapshots and templates.
    /// Call it between cells.
    ///
    /// An untyped function ([`HostSignature::any`]) reports its arguments
    /// as given. A typed one binds them as a Python function would (CPython's
    /// `TypeError`s for missing, extra or unknown arguments), then raises
    /// `TypeError` in place for a parameter whose checkable annotation (see
    /// [`HostParam::annotation`]) does not match; otherwise its
    /// [`crate::Progress::HostCall`] carries every parameter positionally,
    /// in order, defaults filled, and no `kwargs`. A host class is an
    /// immutable value with named fields (a `tuple` subclass, like a named
    /// tuple) whose methods are typed host functions; an instance crosses
    /// the boundary as [`HostValue::Object`].
    ///
    /// The engine has no ambient I/O: the stdlib's OS operations call
    /// untyped builtins (functions of module `""`) under these names, and
    /// raise a `PermissionError` (filesystem) or `RuntimeError` (anything
    /// else) naming the function when none is defined. Arguments are path
    /// strings unless noted: `open(path, mode, ...)`; `Path.exists`,
    /// `Path.is_file`, `Path.is_dir`, `Path.is_symlink`, `Path.read_text`,
    /// `Path.read_bytes`, `Path.stat` (a 10-item `os.stat_result`
    /// sequence), `Path.iterdir` (paths), `Path.resolve`, `Path.unlink`,
    /// `Path.rmdir`, `Path.write_text(path, str)`, `Path.write_bytes(path,
    /// bytes)`, `Path.mkdir(path, parents, exist_ok)`, `Path.rename(src,
    /// dst)`, `Path.scan(path, kind, pattern)` (`kind` `glob`/`rglob`:
    /// matching paths; `walk`: `(dirpath, dirnames, filenames)` triples;
    /// `scandir`: entries), `os.path.ismount`; `os.getenv(key, default)`,
    /// `os.environ()` (a dict), `os.putenv`, `os.unsetenv`, `os.getcwd()`,
    /// `os.chdir`, `os.system`. Defining `os.urandom(n)`, `time.sleep(secs)`
    /// or `asyncio.sleep(delay[, result])` replaces the built-in entropy,
    /// virtual sleep and virtual async sleep.
    ///
    /// Returns a [`HostFunctionRef`] per host function: its call name (what
    /// [`crate::Progress::HostCall`] reports) and id, functions first, then
    /// each class's methods. An id is stable per call name for the
    /// session's lifetime: redefining keeps it, and snapshots and
    /// templates carry it.
    ///
    /// # Errors
    ///
    /// [`DefineError::Invalid`] for an invalid, repeated or reserved name,
    /// an invalid parameter list or a value nested more than 1000
    /// containers deep; [`DefineError::Busy`] while a cell is stopped;
    /// [`DefineError::Raised`] with a `MemoryError` if the definition does
    /// not fit. The previous definition then stays.
    pub fn define(&mut self, module: &HostModule) -> Result<Vec<HostFunctionRef>, DefineError> {
        let name = module.name.as_str();
        if !self.stop.is_empty() {
            return Err(DefineError::Busy);
        }
        check_names(module)?;
        let Generated {
            src,
            raws,
            messages,
            data,
            recipe,
        } = generate(module, &|| self.other_classes(name))?;
        // The spec (what `help` and stubs read, kept with snapshots), made
        // before the code only if the code is compiled now.
        let spec = std::cell::OnceCell::new();
        let program =
            if raws.is_empty() && module.classes.is_empty() && module.exceptions.is_empty() {
                None
            } else {
                Some(host_program(
                    self.vm.module_cache(),
                    name,
                    &src,
                    recipe,
                    &|recipe| {
                        let spec = spec.get_or_init(|| encode_module(module));
                        HostProgram::definition_hash(name, spec, recipe)
                    },
                )?)
            };
        let spec = spec.into_inner().unwrap_or_else(|| encode_module(module));
        let attrs: Vec<&str> = module
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .chain(module.classes.iter().map(|c| c.name.as_str()))
            .chain(module.exceptions.iter().map(|e| e.name.as_str()))
            .collect();
        let constants: Vec<(&str, HostValue)> = module
            .constants
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect();
        let calls: Vec<String> = module
            .functions
            .iter()
            .map(|f| key(name, &f.name))
            .chain(module.classes.iter().flat_map(|c| {
                c.methods
                    .iter()
                    .map(|m| key(name, &format!("{}.{}", c.name, m.name)))
            }))
            .collect();
        self.guarded(|s| {
            let vm = &mut s.vm;
            let defined = (|| {
                let raws: Vec<(&str, &str)> =
                    raws.iter().map(|(c, n)| (c.as_str(), n.as_str())).collect();
                vm.run_host_defs(
                    name,
                    program.as_ref(),
                    &raws,
                    &HostValue::List(messages),
                    &HostValue::List(data),
                )?;
                vm.define_host_module(name, &attrs, &constants, &spec, program.as_ref())?;
                let mut ids = Vec::with_capacity(calls.len());
                for c in calls {
                    ids.push(HostFunctionRef::new(c.clone(), vm.host_id(&c)?));
                }
                Ok(ids)
            })();
            defined.map_err(|_: mm_engine::Raised| DefineError::Raised(s.exception()))
        })
        .unwrap_or_else(|e| Err(DefineError::Poisoned(e.message)))
    }

    /// The classes of the other host modules, as `_host_defs` keys them:
    /// what [`generate`] may refer to for `module`.
    fn other_classes(&self, module: &str) -> Vec<String> {
        self.vm
            .host_module_specs()
            .into_iter()
            .filter(|(m, _, _)| m != module)
            .filter_map(|(_, _, spec)| decode_module(&spec))
            .flat_map(|m| {
                m.classes
                    .iter()
                    .map(|c| key(&m.name, &c.name))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Every host module defined, as given to [`Session::define`] (the
    /// builtins module `""` first, then by name). Restored sessions and
    /// sessions from a template have their definitions too.
    #[must_use]
    pub fn host_modules(&self) -> Vec<HostModule> {
        // Sorted by name as they are decoded, stably: a slice sort of
        // `HostModule`s costs about 8 KB in the guest [M].
        let mut out: Vec<HostModule> = Vec::new();
        for (_, constants, spec) in self.vm.host_module_specs() {
            if let Some(mut m) = decode_module(&spec) {
                m.constants = constants;
                let at = out.partition_point(|o| o.name <= m.name);
                out.insert(at, m);
            }
        }
        out
    }

    /// The id of host function `call_name` (as
    /// [`crate::Progress::HostCall`] reports it), if it was ever defined in
    /// this session.
    #[must_use]
    pub fn host_function_id(&self, call_name: &str) -> Option<u32> {
        self.vm.find_host_id(call_name)
    }

    /// The call name of host function id `id`.
    #[must_use]
    pub fn host_function_name(&self, id: u32) -> Option<&str> {
        self.vm.host_id_name(id)
    }

    /// `.pyi` stubs for everything the host defined, as `(module, text)`
    /// pairs: first `""` for the builtins, then one per host module, by
    /// name. Each lists the constants, classes and functions in definition
    /// order; untyped functions take `*args: Any, **kwargs: Any`. The OS
    /// hooks (dotted builtins) are not guest-visible names and are left
    /// out. For the model's prompt or an external type checker; the engine
    /// itself checks only at the call.
    #[must_use]
    pub fn stubs(&self) -> Vec<(String, String)> {
        let mut modules = self.host_modules();
        if modules.first().is_none_or(|m| !m.name.is_empty()) {
            modules.insert(0, HostModule::default());
        }
        modules
            .into_iter()
            .map(|m| {
                let text = module_stub(&m);
                (m.name, text)
            })
            .collect()
    }
}

/// Appends `module`'s exception classes to the program inside
/// `_host_make`, with their docstrings after the classes' in `docs`.
fn write_exceptions(
    src: &mut String,
    make: &mut String,
    module: &HostModule,
    docs: &mut Vec<HostValue>,
) {
    let name = module.name.as_str();
    for (k, e) in module.exceptions.iter().enumerate() {
        let j = module.classes.len() + k;
        let _ = writeln!(src, "def _host_exc{k}(_host_data):");
        let _ = writeln!(src, "    class {}({}):", e.name, e.base.name());
        let _ = writeln!(src, "        __module__ = _host_module");
        let _ = writeln!(src, "        __qualname__ = '{}'", e.name);
        src.push_str("        tool = None\n        data = None\n");
        let _ = writeln!(src, "    {}.__doc__ = _host_docs[{j}] or None", e.name);
        let _ = writeln!(src, "    return {}", e.name);
        let _ = writeln!(
            make,
            "    _host_new['{}'] = _host_exc{k}(_host_data)",
            key(name, &e.name)
        );
        docs.push(HostValue::Str(e.doc.clone()));
    }
}

/// Rejects invalid or repeated names in `module` before anything runs.
fn check_names(module: &HostModule) -> Result<(), DefineError> {
    let name = module.name.as_str();
    if !name.is_empty() {
        for part in name.split('.') {
            check_ident("module", part)?;
        }
        // `import` finds micro-monty's own modules first, so a host module
        // of such a name could never be reached.
        if name == "builtins"
            || mm_engine::is_bundled(name)
            || mm_stdlib::MODULES.iter().any(|m| m.name == name)
        {
            return Err(value_error(&format!(
                "cannot define host module {}: micro-monty has a module of that name, \
                 which `import` finds first; choose another name",
                debug_quoted(name)
            )));
        }
    }
    for (k, v) in &module.constants {
        if too_deep(v) {
            return Err(value_error(&format!(
                "constant {} nests deeper than {MAX_VALUE_DEPTH} containers",
                debug_quoted(k)
            )));
        }
    }
    let defaults = module
        .functions
        .iter()
        .chain(module.classes.iter().flat_map(|c| c.methods.iter()))
        .flat_map(|f| f.params.iter().map(move |p| (f, p)));
    for (f, p) in defaults {
        if p.default.as_ref().is_some_and(too_deep) {
            return Err(value_error(&format!(
                "the default of {}'s parameter {} nests deeper than {MAX_VALUE_DEPTH} containers",
                f.name,
                debug_quoted(&p.name)
            )));
        }
    }
    for f in &module.functions {
        let hook = name.is_empty() && f.is_untyped();
        for part in f.name.split('.').take(if hook { usize::MAX } else { 1 }) {
            check_ident("function", part)?;
        }
        if !hook && f.name.contains('.') {
            return Err(value_error(&format!(
                "invalid function name {}",
                debug_quoted(&f.name)
            )));
        }
    }
    for (k, _) in &module.constants {
        check_ident("constant", k)?;
    }
    let attrs: Vec<&str> = module
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .chain(module.classes.iter().map(|c| c.name.as_str()))
        .chain(module.exceptions.iter().map(|e| e.name.as_str()))
        .collect();
    let names = attrs
        .iter()
        .copied()
        .chain(module.constants.iter().map(|(k, _)| k.as_str()));
    for (i, n) in names.clone().enumerate() {
        if names.clone().take(i).any(|m| m == n) {
            return Err(value_error(&format!(
                "{} is defined twice",
                debug_quoted(n)
            )));
        }
    }
    for c in &module.classes {
        check_ident("class", &c.name)?;
    }
    for e in &module.exceptions {
        check_ident("exception", &e.name)?;
    }
    Ok(())
}

/// The program [`Session::define`] runs and what it binds.
struct Generated {
    src: String,
    /// `(call name, name)` of each plain host function it wraps.
    raws: Vec<(String, String)>,
    messages: Vec<HostValue>,
    data: Vec<HostValue>,
    /// The other modules' classes `src` refers to, in order of first use:
    /// with the module, all [`generate`] needs to write `src` again.
    recipe: Vec<String>,
}

/// `module` (without its constants, which the engine keeps itself) as
/// the plain value the engine stores for [`decode_module`].
fn encode_module(module: &HostModule) -> HostValue {
    let s = |t: &str| HostValue::Str(t.to_owned());
    let sig = |f: &HostSignature| {
        let params = f
            .params
            .iter()
            .map(|p| {
                let kind = match p.kind {
                    ParamKind::PositionalOrKeyword => 0,
                    ParamKind::KeywordOnly => 1,
                    ParamKind::VarPositional => 2,
                    ParamKind::VarKeyword => 3,
                };
                HostValue::List(vec![
                    s(&p.name),
                    s(&p.annotation),
                    HostValue::Int(kind),
                    HostValue::List(p.default.iter().cloned().collect()),
                ])
            })
            .collect();
        HostValue::List(vec![
            s(&f.name),
            HostValue::List(params),
            s(&f.returns),
            s(&f.doc),
            HostValue::Bool(f.dict_as_kwargs),
        ])
    };
    let classes = module
        .classes
        .iter()
        .map(|c| {
            let fields = c
                .fields
                .iter()
                .map(|(f, a)| HostValue::List(vec![s(f), s(a)]))
                .collect();
            HostValue::List(vec![
                s(&c.name),
                HostValue::List(fields),
                HostValue::List(c.methods.iter().map(sig).collect()),
                s(&c.doc),
            ])
        })
        .collect();
    let exceptions = module
        .exceptions
        .iter()
        .map(|e| HostValue::List(vec![s(&e.name), s(e.base.name()), s(&e.doc)]))
        .collect();
    // `help()` and `__doc__` read `[stub, doc]` per attribute ("" is the
    // module), the last item, in the engine.
    let entry = |stub: String, doc: &str| HostValue::List(vec![HostValue::Str(stub), s(doc)]);
    let mut help = vec![(String::new(), entry(module_stub(module), ""))];
    for f in module.functions.iter().filter(|f| !f.name.contains('.')) {
        let callable = Callable {
            sig: f,
            shown: f.name.clone(),
            receiver: None,
        };
        help.push((f.name.clone(), entry(callable.stub(""), &f.doc)));
    }
    for c in &module.classes {
        help.push((c.name.clone(), entry(class_stub(c), &c.doc)));
        for m in &c.methods {
            let callable = Callable {
                sig: m,
                shown: String::new(),
                receiver: Some("self"),
            };
            help.push((
                format!("{}.{}", c.name, m.name),
                entry(callable.stub(""), &m.doc),
            ));
        }
    }
    for e in &module.exceptions {
        help.push((e.name.clone(), entry(exception_stub(e), &e.doc)));
    }
    HostValue::List(vec![
        s(&module.name),
        HostValue::List(module.functions.iter().map(sig).collect()),
        HostValue::List(classes),
        HostValue::List(exceptions),
        HostValue::Dict(help),
    ])
}

/// The `.pyi` text of `module`.
fn module_stub(m: &HostModule) -> String {
    let mut text = String::from("from typing import Any\n\n");
    for (k, v) in &m.constants {
        let _ = writeln!(text, "{k}: {}", type_of(v));
    }
    for e in &m.exceptions {
        text.push_str(&exception_stub(e));
    }
    for c in &m.classes {
        text.push_str(&class_stub(c));
    }
    for f in m.functions.iter().filter(|f| !f.name.contains('.')) {
        let callable = Callable {
            sig: f,
            shown: f.name.clone(),
            receiver: None,
        };
        text.push_str(&callable.stub(""));
    }
    text
}

/// The stub of host exception class `e`.
fn exception_stub(e: &HostException) -> String {
    let mut stub = format!("class {}({}):\n", e.name, e.base.name());
    if !e.doc.is_empty() {
        let _ = writeln!(stub, "    {}", docstring(&e.doc));
    }
    stub.push_str("    tool: str | None\n    data: Any\n");
    stub
}

/// The [`HostModule`] [`encode_module`] made (no constants).
fn decode_module(v: &HostValue) -> Option<HostModule> {
    fn list(v: &HostValue) -> Option<&[HostValue]> {
        match v {
            HostValue::List(items) => Some(items),
            _ => None,
        }
    }
    fn s(v: &HostValue) -> Option<String> {
        match v {
            HostValue::Str(t) => Some(t.clone()),
            _ => None,
        }
    }
    fn sig(v: &HostValue) -> Option<HostSignature> {
        let [name, params, returns, doc, dict_as_kwargs] = list(v)? else {
            return None;
        };
        let params = list(params)?
            .iter()
            .map(|p| {
                let [name, annotation, kind, default] = list(p)? else {
                    return None;
                };
                Some(HostParam {
                    name: s(name)?,
                    annotation: s(annotation)?,
                    kind: match kind {
                        HostValue::Int(0) => ParamKind::PositionalOrKeyword,
                        HostValue::Int(1) => ParamKind::KeywordOnly,
                        HostValue::Int(2) => ParamKind::VarPositional,
                        _ => ParamKind::VarKeyword,
                    },
                    default: list(default)?.first().cloned(),
                })
            })
            .collect::<Option<_>>()?;
        Some(HostSignature {
            name: s(name)?,
            params,
            returns: s(returns)?,
            doc: s(doc)?,
            dict_as_kwargs: matches!(dict_as_kwargs, HostValue::Bool(true)),
        })
    }
    let [name, functions, classes, exceptions, _help] = list(v)? else {
        return None;
    };
    let exceptions = list(exceptions)?
        .iter()
        .map(|e| match list(e)? {
            [name, base, doc] => Some(HostException {
                name: s(name)?,
                base: HostErrorKind::from_name(&s(base)?)?,
                doc: s(doc)?,
            }),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let classes = list(classes)?
        .iter()
        .map(|c| {
            let [name, fields, methods, doc] = list(c)? else {
                return None;
            };
            let fields = list(fields)?
                .iter()
                .map(|f| match list(f)? {
                    [f, a] => Some((s(f)?, s(a)?)),
                    _ => None,
                })
                .collect::<Option<_>>()?;
            Some(HostClass {
                name: s(name)?,
                fields,
                methods: list(methods)?.iter().map(sig).collect::<Option<_>>()?,
                doc: s(doc)?,
            })
        })
        .collect::<Option<_>>()?;
    Some(HostModule {
        name: s(name)?,
        functions: list(functions)?.iter().map(sig).collect::<Option<_>>()?,
        classes,
        constants: Vec::new(),
        exceptions,
    })
}

/// The `class` statement of host class `class`, inside its
/// `_host_class{j}` helper: fields, constructor, accessors, `repr` and
/// immutability.
fn class_body_at(class: &HostClass, module: &str, base: &str) -> String {
    let fields: Vec<&str> = class.fields.iter().map(|(f, _)| f.as_str()).collect();
    let tuple = |names: &[&str]| {
        let mut t = names
            .iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(", ");
        if names.len() == 1 {
            t.push(',');
        }
        format!("({t})")
    };
    let mut src = String::new();
    let _ = writeln!(src, "    class {base}(tuple):");
    let _ = writeln!(
        src,
        "        __module__ = '{}'",
        if module.is_empty() {
            "builtins"
        } else {
            module
        }
    );
    let _ = writeln!(src, "        __qualname__ = '{base}'");
    let _ = writeln!(src, "        _fields = {}", tuple(&fields));
    src.push_str("        __match_args__ = _fields\n");
    let _ = writeln!(
        src,
        "        __host_class__ = '{}'",
        key(module, &class.name)
    );
    let params: Vec<&str> = std::iter::once("_host_cls")
        .chain(fields.iter().copied())
        .collect();
    let _ = writeln!(src, "        def __new__({}):", params.join(", "));
    let comma = if fields.len() == 1 { "," } else { "" };
    let _ = writeln!(
        src,
        "            return tuple.__new__(_host_cls, ({}{comma}))",
        fields.join(", ")
    );
    for (i, f) in fields.iter().enumerate() {
        let _ = writeln!(
            src,
            "        {f} = property(lambda _host_s: tuple.__getitem__(_host_s, {i}))"
        );
    }
    src.push_str("        def __repr__(self):\n");
    let parts: Vec<String> = fields
        .iter()
        .enumerate()
        .map(|(i, f)| format!("'{f}=' + repr(tuple.__getitem__(self, {i}))"))
        .collect();
    let joined = if parts.is_empty() {
        "''".to_owned()
    } else {
        parts.join(" + ', ' + ")
    };
    let _ = writeln!(src, "            return '{base}(' + {joined} + ')'");
    src.push_str("        def __setattr__(self, name, value):\n");
    src.push_str("            raise AttributeError(f'cannot assign to field {name!r}')\n");
    src.push_str("        def __delattr__(self, name):\n");
    src.push_str("            raise AttributeError(f'cannot delete field {name!r}')\n");
    src
}

/// The stub of host class `class` (`base` without its module).
fn class_stub(class: &HostClass) -> String {
    let mut stub = format!("class {}:\n", class.name);
    if !class.doc.is_empty() {
        let _ = writeln!(stub, "    {}", docstring(&class.doc));
    }
    for (f, a) in &class.fields {
        let _ = writeln!(stub, "    {f}: {}", if a.is_empty() { "Any" } else { a });
    }
    let init: Vec<String> = class
        .fields
        .iter()
        .map(|(f, a)| format!("{f}: {}", if a.is_empty() { "Any" } else { a }))
        .collect();
    let _ = writeln!(
        stub,
        "    def __init__(self, {}) -> None: ...",
        init.join(", ")
    );
    for sig in &class.methods {
        let m = Callable {
            sig,
            shown: String::new(),
            receiver: Some("self"),
        };
        stub.push_str(&m.stub("    "));
    }
    stub
}

/// A host value's type for a stub.
fn type_of(v: &HostValue) -> &str {
    match v {
        HostValue::None => "None",
        HostValue::Bool(_) => "bool",
        HostValue::Int(_) | HostValue::BigInt(_) => "int",
        HostValue::Float(_) => "float",
        HostValue::Str(_) => "str",
        HostValue::Bytes(_) => "bytes",
        HostValue::List(_) => "list[Any]",
        HostValue::Dict(_) => "dict[str, Any]",
        HostValue::Record { .. } | HostValue::Tuple(_) => "tuple[Any, ...]",
        HostValue::Object { class, .. } => class,
        _ => "Any",
    }
}

/// The program defining `module`'s classes and typed functions. Each
/// class, exception and typed function is built by its own top-level
/// helper, so no function's locals grow with the module (a function
/// holds at most 250).
/// `others` lists the other modules' classes; it is called only for an
/// annotation that names none of the module's own.
fn generate(
    module: &HostModule,
    others: &dyn Fn() -> Vec<String>,
) -> Result<Generated, DefineError> {
    const ARGS: &str = "(_host_raw, _host_msgs, _host_data, _host_new)";
    let name = module.name.as_str();
    let classes: Vec<String> = module.classes.iter().map(|c| key(name, &c.name)).collect();
    // The other modules' classes it refers to: the recipe.
    let used = std::cell::RefCell::new(Vec::new());
    let others_once = std::cell::OnceCell::new();
    let others = || others_once.get_or_init(others).as_slice();
    let class_expr = |annotation: &str| class_expr(annotation, name, &classes, &others, &used);

    let mut src = String::from(PRELUDE);
    let shown_module = if name.is_empty() { "builtins" } else { name };
    let _ = writeln!(src, "_host_module = '{shown_module}'");
    // The body of `_host_make`.
    let mut make =
        String::from("def _host_make(_host_raw, _host_msgs, _host_data):\n    _host_new = {}\n");
    let mut raws: Vec<(String, String)> = Vec::new();
    let mut messages = Vec::new();
    let mut data = Vec::new();
    let mut docs = Vec::new();
    // `def _host_fn{i}`: builds callable `i`.
    let mut write_fn = |src: &mut String, c: &Callable<'_>, raws: &mut Vec<(String, String)>| {
        let i = raws.len();
        let _ = writeln!(src, "def _host_fn{i}{ARGS}:");
        c.write_impl(
            src,
            "    ",
            i,
            &format!("_host_raw[{i}]"),
            &mut messages,
            &class_expr,
        );
        let _ = writeln!(src, "    return {}", c.signature_call(i, &c.shown));
        raws.push((key(name, &c.shown), c.sig.name.clone()));
        data.push(c.data());
        i
    };
    for (j, class) in module.classes.iter().enumerate() {
        let base = class.name.as_str();
        for (f, _) in &class.fields {
            check_ident("field", f)?;
        }
        let mut methods = Vec::new();
        for m in &class.methods {
            check_ident("method", &m.name)?;
            if class.fields.iter().any(|(f, _)| *f == m.name) {
                return Err(value_error(&format!(
                    "method {} has a field's name",
                    debug_quoted(&m.name)
                )));
            }
            let callable = Callable {
                sig: m,
                shown: format!("{base}.{}", m.name),
                receiver: Some("self"),
            };
            callable.check()?;
            methods.push(write_fn(&mut src, &callable, &mut raws));
        }
        let _ = writeln!(src, "def _host_class{j}{ARGS}:");
        src.push_str(&class_body_at(class, name, base));
        for (m, i) in class.methods.iter().zip(methods) {
            let _ = writeln!(src, "    {base}.{} = _host_fn{i}{ARGS}", m.name);
        }
        let _ = writeln!(src, "    {base}.__doc__ = _host_docs[{j}] or None");
        let _ = writeln!(src, "    return {base}");
        let _ = writeln!(
            make,
            "    _host_new['{}'] = _host_class{j}{ARGS}",
            classes[j]
        );
        docs.push(HostValue::Str(class.doc.clone()));
    }
    write_exceptions(&mut src, &mut make, module, &mut docs);
    for f in &module.functions {
        if f.is_untyped() {
            continue;
        }
        let callable = Callable {
            sig: f,
            shown: f.name.clone(),
            receiver: None,
        };
        callable.check()?;
        let i = write_fn(&mut src, &callable, &mut raws);
        let _ = writeln!(
            make,
            "    _host_new['{}'] = _host_fn{i}{ARGS}",
            key(name, &f.name)
        );
    }
    src.push_str(&make);
    src.push_str("    return _host_new\n_host_make(_host_raw, _host_msgs, _host_data)\n");
    // `_host_data` is `[per callable..., class docs]`.
    let src = src.replace("_host_docs[", &format!("_host_data[{} + ", data.len()));
    data.extend(docs);
    let recipe = used.into_inner();
    Ok(Generated {
        src,
        raws,
        messages,
        data,
        recipe,
    })
}

/// The Python expression for host class `annotation` inside generated
/// code for module `module`, if one is defined: one of `own` (the module's
/// classes, keyed as `_host_defs` keys them), else one of `others` (another
/// module's), which goes in `used`.
fn class_expr<'a>(
    annotation: &str,
    module: &str,
    own: &[String],
    others: &dyn Fn() -> &'a [String],
    used: &std::cell::RefCell<Vec<String>>,
) -> Option<String> {
    let full = key(module, annotation);
    if let Some(k) = [&full, annotation]
        .into_iter()
        .find(|k| own.iter().any(|o| o == *k))
    {
        return Some(format!("_host_new['{k}']"));
    }
    let k = [&full, annotation]
        .into_iter()
        .find(|k| others().iter().any(|o| o == *k))?;
    let mut used = used.borrow_mut();
    if !used.iter().any(|u| u == k) {
        used.push(k.to_owned());
    }
    Some(format!("_host_defs['{k}']"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_codegen_probe_matches_its_recorded_hash() {
        let probe = super::probe_code();
        assert!(probe.as_ref().is_ok_and(|b| b.len() > 1000), "{probe:?}");
        let h = probe.map_or(0, |b| mm_core::xxh64(&b, 0));
        assert!(
            h == super::CODEGEN_PROBE,
            "the spec codec, host-module generator or compiler changed the \
             probe's code: set CODEGEN_PROBE to 0x{:04x}_{:04x}_{:04x}_{:04x} \
             (this changes the snapshot build id)",
            h >> 48,
            (h >> 32) & 0xffff,
            (h >> 16) & 0xffff,
            h & 0xffff
        );
    }
}
