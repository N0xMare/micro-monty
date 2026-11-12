//! Exceptions: context chaining, tracebacks as `(code, pc)` pairs, "Did you
//! mean" hints, and CPython 3.14's traceback text with `~~^~~` carets.

use crate::scratch::NativeIds;
use crate::text::char_len;
use crate::types::{T, ty};
use crate::vm::{Raised, Vm, exc_slot};
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::fmt::Write;
use mm_core::Value;

/// The text appended to the report's last line: a str, or the data of a
/// "Did you mean" computed when rendered ([`Vm::hint_of`]), or `None`.
/// The data is a `(kind, name, data, locals, note)` tuple: `name` is
/// `NameError.name` and `AttributeError.name`; `data` the globals dict of
/// the raising frame (NameError), `AttributeError.obj`, or the module an
/// `ImportError` imported from; `locals` the frame's bound names, one per
/// line; `note` a fixed micro-monty sentence shown instead of a suggestion.
const HINT: u32 = 5;
const PENDING_NAME: i64 = 0;
const PENDING_ATTR: i64 = 1;

/// Line byte ranges of each source a traceback shows ([`Vm::line_ranges`]).
type LineCache = Vec<(u32, Vec<(usize, usize)>)>;
const PENDING_MODULE: i64 = 2;
/// The `locals` of a `NameError`'s hint data when the raising frame's
/// `self` has the missing name: CPython suggests `self.name`.
pub(crate) const SELF_HAS_NAME: &str = "\0self";
/// `ImportError`'s keyword arguments (and attributes besides `msg`).
const IMPORT_ATTRS: [&str; 3] = ["name", "path", "name_from"];
const RECURSIVE_CUTOFF: usize = 3;
const MAX_CANDIDATES: usize = 750;
const MAX_STRING: usize = 40;
const MOVE_COST: usize = 2;
const CASE_COST: usize = 1;

/// Common stdlib module names, for "Did you forget to import 'x'?".
const STDLIB: &[&str] = &[
    "abc",
    "argparse",
    "array",
    "asyncio",
    "base64",
    "bisect",
    "builtins",
    "calendar",
    "cmath",
    "codecs",
    "collections",
    "contextlib",
    "copy",
    "csv",
    "dataclasses",
    "datetime",
    "decimal",
    "difflib",
    "enum",
    "fractions",
    "functools",
    "glob",
    "gzip",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "inspect",
    "io",
    "itertools",
    "json",
    "logging",
    "math",
    "operator",
    "os",
    "pathlib",
    "pickle",
    "platform",
    "pprint",
    "queue",
    "random",
    "re",
    "secrets",
    "shlex",
    "shutil",
    "signal",
    "socket",
    "sqlite3",
    "statistics",
    "string",
    "struct",
    "subprocess",
    "sys",
    "tempfile",
    "textwrap",
    "threading",
    "time",
    "timeit",
    "traceback",
    "types",
    "typing",
    "unicodedata",
    "unittest",
    "urllib",
    "uuid",
    "warnings",
    "weakref",
    "xml",
    "zipfile",
    "zlib",
];

fn sub_cost(a: char, b: char) -> usize {
    if a == b {
        0
    } else if a.to_lowercase().eq(b.to_lowercase()) {
        CASE_COST
    } else {
        MOVE_COST
    }
}

/// CPython's `levenshtein_distance` (Python/suggestions.c).
fn levenshtein(a: &str, b: &str, max_cost: usize) -> usize {
    if a == b {
        return 0;
    }
    // The common ends first, without copying: a name can be as long as
    // the memory limit allows, and only a short middle is compared.
    let (na, nb) = (a.chars().count(), b.chars().count());
    let pre = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
    let post = a
        .chars()
        .rev()
        .zip(b.chars().rev())
        .take(na.min(nb) - pre)
        .take_while(|(x, y)| x == y)
        .count();
    let (ra, rb) = (na - pre - post, nb - pre - post);
    if ra == 0 || rb == 0 {
        return MOVE_COST * (ra + rb);
    }
    if ra > MAX_STRING || rb > MAX_STRING {
        return max_cost + 1;
    }
    let mut a: Vec<char> = a.chars().skip(pre).take(ra).collect();
    let mut b: Vec<char> = b.chars().skip(pre).take(rb).collect();
    if b.len() < a.len() {
        core::mem::swap(&mut a, &mut b);
    }
    if (b.len() - a.len()) * MOVE_COST > max_cost {
        return max_cost + 1;
    }
    let mut row: Vec<usize> = (1..=a.len()).map(|i| i * MOVE_COST).collect();
    let mut result = 0;
    for (bi, &bc) in b.iter().enumerate() {
        let mut distance = bi * MOVE_COST;
        result = distance;
        let mut minimum = usize::MAX;
        for (i, &ac) in a.iter().enumerate() {
            let substitute = distance + sub_cost(bc, ac);
            distance = row[i];
            let insert_delete = result.min(distance) + MOVE_COST;
            result = insert_delete.min(substitute);
            row[i] = result;
            minimum = minimum.min(result);
        }
        if minimum > max_cost {
            return max_cost + 1;
        }
    }
    result
}

/// The closest candidate to `wrong`, as CPython suggests it.
pub(crate) fn suggest<'a>(candidates: &[&'a str], wrong: &str) -> Option<&'a str> {
    if candidates.len() > MAX_CANDIDATES {
        return None;
    }
    let wlen = char_len(wrong);
    if wlen > MAX_STRING {
        return None;
    }
    // CPython's C `_suggestions` starts from "no limit" (traceback.py's
    // pure-Python fallback starts from the name length).
    let mut best = usize::MAX;
    let mut out: Option<&str> = None;
    for &c in candidates {
        if c == wrong {
            continue;
        }
        let max = ((char_len(c) + wlen + 3) * MOVE_COST / 6).min(best.saturating_sub(1));
        let d = levenshtein(wrong, c, max);
        if d > max {
            continue;
        }
        if out.is_none() || d < best {
            out = Some(c);
            best = d;
        }
    }
    out
}

/// East Asian wide characters take two columns in a terminal.
fn char_width(c: char) -> usize {
    let n = c as u32;
    let wide = matches!(n,
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6 | 0x1F300..=0x1F64F | 0x1F900..=0x1F9FF | 0x20000..=0x3FFFD);
    if wide { 2 } else { 1 }
}

fn display_width(line: &str, offset: usize) -> usize {
    if line.is_ascii() {
        return offset;
    }
    line.chars().take(offset).map(char_width).sum()
}

/// `textwrap.dedent`.
fn dedent(lines: &[String]) -> Vec<String> {
    let mut margin: Option<String> = None;
    for l in lines {
        if l.trim().is_empty() {
            continue;
        }
        let ws: String = l.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        margin = Some(match margin {
            None => ws,
            Some(m) => m
                .chars()
                .zip(ws.chars())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect(),
        });
    }
    let m = margin.unwrap_or_default();
    lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                l.strip_prefix(m.as_str()).unwrap_or(l).to_string()
            }
        })
        .collect()
}

impl Vm {
    /// The `T` of an exception instance.
    pub(crate) fn exc_type(&self, e: Value) -> T {
        self.type_of(e)
    }

    /// Instantiates exception type `t` with `args`.
    pub(crate) fn new_exception(&mut self, t: T, args: &[Value]) -> Result<Value, Raised> {
        if matches!(t, T::NameError | T::UnboundLocalError) {
            self.name_error_built = true;
        }
        let tv = self.type_obj(t);
        let a = self.alloc_tuple(args)?;
        self.alloc_exc(tv, a)
    }

    /// Sets `exc.__context__` to the exception being handled (CPython's
    /// `_PyErr_SetObject`), breaking any cycle that would create.
    pub(crate) fn attach_context(&mut self, exc: Value) {
        if exc == self.memory_error {
            self.fresh_memory_error();
        }
        let h = self.handled;
        if h == Value::NONE || h == exc || !exc.is_obj_kind(ty::EXC as u8) {
            return;
        }
        let mut o = h;
        for _ in 0..10_000 {
            let c = self.vslot(o, exc_slot::CONTEXT);
            if c == Value::NONE || c == Value::UNDEFINED {
                break;
            }
            if c == exc {
                if let Some(r) = o.as_obj() {
                    self.set_slot(r, exc_slot::CONTEXT, Value::NONE);
                }
                break;
            }
            o = c;
        }
        if let Some(r) = exc.as_obj() {
            self.set_slot(r, exc_slot::CONTEXT, h);
        }
    }

    /// Clears the shared `MemoryError`, raised when the heap has no room
    /// for a new one, as each raise begins: an earlier raise's traceback,
    /// context, hint and attributes do not carry over, as CPython takes a
    /// fresh one from its free list. A cause the raise itself set (`raise
    /// e from x`) stays.
    #[cold]
    #[inline(never)]
    fn fresh_memory_error(&mut self) {
        if let Some(r) = self.memory_error.as_obj() {
            let _ = self
                .heap
                .set_values(r, exc_slot::CONTEXT, &[Value::NONE; 5]);
        }
    }

    /// Appends a traceback entry `(ci, pc)`.
    pub(crate) fn add_tb(&mut self, exc: Value, ci: u32, pc: u32) {
        if self.codes.get(ci as usize).is_some_and(|c| c.prelude) {
            return;
        }
        let Some(r) = exc.as_obj() else { return };
        if !exc.is_obj_kind(ty::EXC as u8) {
            return;
        }
        let mut tb = self.slot(r, exc_slot::TRACEBACK);
        if !tb.is_obj_kind(ty::LIST as u8) {
            match self.new_list(&[]) {
                Ok(l) => {
                    tb = l;
                    self.set_slot(r, exc_slot::TRACEBACK, l);
                }
                Err(Raised) => return,
            }
        }
        let entry = (i64::from(ci) << 28) | i64::from(pc & 0x0FFF_FFFF);
        if let Some(v) = Value::int(entry) {
            let saved = self.exc;
            let _ = self.list_push(tb, v);
            self.exc = saved;
        }
    }

    pub(crate) fn set_hint_suffix(&mut self, hint: &str) {
        let e = self.exc;
        if let Ok(s) = self.new_str(hint)
            && let Some(r) = e.as_obj()
            && e.is_obj_kind(ty::EXC as u8)
        {
            self.set_slot_o(r, HINT, s);
        }
        self.exc = e;
    }

    /// Records the hint data on the exception being raised: the
    /// suggestion is computed by [`Vm::hint_of`] only if the report is
    /// rendered, as CPython's traceback does, so a caught error costs
    /// nothing.
    fn set_hint_pending(
        &mut self,
        kind: i64,
        name: Value,
        data: Value,
        locals: Value,
        note: Value,
    ) {
        let e = self.exc;
        if let Some(r) = e.as_obj()
            && e.is_obj_kind(ty::EXC as u8)
            && let Some(k) = Value::int(kind)
            && let Ok(t) = self.alloc_tuple(&[k, name, data, locals, note])
        {
            self.set_slot_o(r, HINT, t);
        }
        self.exc = e;
    }

    /// The hint data of `e` (see [`HINT`]), when it is of `kind`.
    fn hint_data(&self, e: Value, kind: i64) -> Option<Vec<Value>> {
        let h = self.vslot_o(e, HINT);
        if !h.is_obj_kind(ty::TUPLE as u8) {
            return None;
        }
        let p = self.tuple_items(h);
        (p.len() == 5 && p[0].as_int() == Some(kind)).then_some(p)
    }

    /// The kind of hint data a `NameError` (`PENDING_NAME`) or an
    /// `AttributeError` (`PENDING_ATTR`) instance carries, else `None`.
    fn hint_kind(&self, e: Value) -> Option<i64> {
        // The builtin base decides, by a constant match rather than a walk
        // up the bases (every `raise` asks): `UnboundLocalError` is the
        // only builtin subclass of either.
        match self.class_base_t(self.vslot_o(e, exc_slot::TYPE)) {
            T::NameError | T::UnboundLocalError => Some(PENDING_NAME),
            T::AttributeError => Some(PENDING_ATTR),
            _ => None,
        }
    }

    /// Is `e` an `ImportError` (or a subclass)?
    pub(crate) fn is_import_error(&self, e: Value) -> bool {
        matches!(
            self.class_base_t(self.vslot_o(e, exc_slot::TYPE)),
            T::ImportError | T::ModuleNotFoundError
        )
    }

    /// An `ImportError`'s `name`, `path`, `name_from` or `msg` when its
    /// instance dict has none: `msg` is the one argument, the rest `None`.
    pub(crate) fn import_attr(&self, e: Value, attr: &str) -> Value {
        let args = self.tuple_items(self.vslot_o(e, exc_slot::ARGS));
        match (attr, &args[..]) {
            ("msg", [m]) => *m,
            _ => Value::NONE,
        }
    }

    /// Sets `name` (and `name_from`, unless `UNDEFINED`) on the
    /// `ImportError` being raised.
    pub(crate) fn set_import_names(&mut self, name: &str, from: Value) {
        let e = self.exc;
        let set = |vm: &mut Vm, k: &str, v: Value| -> Result<(), Raised> {
            let k = vm.intern(k)?;
            vm.inst_set(e, k, v).map(drop)
        };
        if let Ok(n) = self.new_str(name) {
            let _ = set(self, "name", n);
        }
        if from != Value::UNDEFINED {
            let _ = set(self, "name_from", from);
        }
        self.exc = e;
    }

    /// A `SyntaxError`'s `msg`, `filename`, `lineno`, `offset`, `text`,
    /// `end_lineno`, `end_offset` or `print_file_and_line`, as CPython's
    /// constructor reads them from `(msg, (filename, lineno, offset, text[,
    /// end_lineno, end_offset]))`; an assigned value comes first.
    pub(crate) fn syntax_attr(&self, e: Value, attr: &str) -> Value {
        if let Some(k) = self.find_interned(attr)
            && let Some(v) = self.inst_get(e, k)
        {
            return v;
        }
        let args = self.tuple_items(self.vslot_o(e, exc_slot::ARGS));
        if attr == "msg" {
            return args.first().copied().unwrap_or(Value::NONE);
        }
        let info = match &args[..] {
            [_, i] if i.is_obj_kind(ty::TUPLE as u8) => self.tuple_items(*i),
            _ => Vec::new(),
        };
        if !(4..=6).contains(&info.len()) {
            return Value::NONE;
        }
        let i = match attr {
            "filename" => 0,
            "lineno" => 1,
            "offset" => 2,
            "text" => 3,
            "end_lineno" => 4,
            "end_offset" => 5,
            _ => 6,
        };
        info.get(i).copied().unwrap_or(Value::NONE)
    }

    /// `str()` of a `SyntaxError` carrying a filename or line number:
    /// `msg (file, line N)` with the file's base name, as CPython's.
    pub(crate) fn syntax_str(&mut self, e: Value) -> Result<Option<String>, Raised> {
        if !self.exc_type(e).is_subclass(T::SyntaxError) {
            return Ok(None);
        }
        let file = self.syntax_attr(e, "filename");
        let line = self.syntax_attr(e, "lineno");
        let file = Vm::is_str(file).then(|| {
            let f = self.str_ref_o(file);
            String::from(f.rsplit('/').next().unwrap_or(f))
        });
        let line = line.as_int();
        if file.is_none() && line.is_none() {
            return Ok(None);
        }
        let msg = self.syntax_attr(e, "msg");
        let mut s = String::from(&*self.stringify(msg)?);
        match (file, line) {
            (Some(f), Some(l)) => {
                let _ = write!(s, " ({f}, line {l})");
            }
            (Some(f), None) => {
                let _ = write!(s, " ({f})");
            }
            (None, l) => {
                let _ = write!(s, " (line {})", l.unwrap_or(0));
            }
        }
        Ok(Some(s))
    }

    /// `NameError.name`, `AttributeError.name` and `AttributeError.obj`;
    /// `None` for other attributes and exceptions.
    pub(crate) fn exc_name_attr(&mut self, e: Value, attr: &str) -> Option<Value> {
        let kind = self.hint_kind(e)?;
        let i = match (attr, kind) {
            ("name", _) => 1,
            ("obj", PENDING_ATTR) => 2,
            _ => return None,
        };
        Some(self.hint_data(e, kind).map_or(Value::NONE, |p| p[i]))
    }

    /// Sets `NameError.name`, `AttributeError.name` or `AttributeError.obj`;
    /// `false` for other attributes and exceptions.
    pub(crate) fn set_exc_name_attr(
        &mut self,
        e: Value,
        attr: &str,
        v: Value,
    ) -> Result<bool, Raised> {
        let Some(kind) = self.hint_kind(e) else {
            return Ok(false);
        };
        let i = match (attr, kind) {
            ("name", _) => 1,
            ("obj", PENDING_ATTR) => 2,
            _ => return Ok(false),
        };
        let mut p = self.hint_data(e, kind).unwrap_or_else(|| {
            vec![
                Value::int(kind).unwrap_or(Value::NONE),
                Value::NONE,
                Value::NONE,
                Value::NONE,
                Value::NONE,
            ]
        });
        p[i] = v;
        if kind == PENDING_NAME {
            self.name_error_built = true;
        }
        let t = self.alloc_tuple(&p)?;
        if let Some(r) = e.as_obj() {
            self.set_slot_o(r, HINT, t);
        }
        Ok(true)
    }

    /// A builtin exception constructor's keyword arguments, as CPython
    /// takes them: `name` for a `NameError`, `name` and `obj` for an
    /// `AttributeError`; none for the others (`tname`, the class called,
    /// says so; an unexpected keyword names the builtin base, as CPython's
    /// argument parser does).
    pub(crate) fn exc_init_kwargs(
        &mut self,
        e: Value,
        tname: &str,
        kws: &[(Value, Value)],
    ) -> Result<(), Raised> {
        if self.is_import_error(e) {
            // `name`, `path` and `name_from` live in the instance dict.
            for &(k, v) in kws {
                if !IMPORT_ATTRS.contains(&self.str_ref_o(k)) {
                    let msg = format!(
                        "ImportError() got an unexpected keyword argument '{}'",
                        self.str_ref_o(k)
                    );
                    return Err(self.err(T::TypeError, &msg));
                }
                self.inst_set(e, k, v)?;
            }
            return Ok(());
        }
        if self.hint_kind(e).is_none() {
            let msg = format!("{tname}() takes no keyword arguments");
            return Err(self.err(T::TypeError, &msg));
        }
        for &(k, v) in kws {
            let n = String::from(self.str_ref_o(k));
            if !self.set_exc_name_attr(e, &n, v)? {
                let base = if self.hint_kind(e) == Some(PENDING_NAME) {
                    "NameError"
                } else {
                    "AttributeError"
                };
                let msg = format!("{base}() got an unexpected keyword argument '{n}'");
                return Err(self.err(T::TypeError, &msg));
            }
        }
        Ok(())
    }

    /// Whether `raise e` should keep the frame's names: a `NameError`
    /// raised for the first time.
    pub(crate) fn wants_raise_names(&self, e: Value) -> bool {
        self.hint_kind(e) == Some(PENDING_NAME)
            && self
                .hint_data(e, PENDING_NAME)
                .is_none_or(|p| p[2] == Value::NONE)
    }

    /// At `raise` of a `NameError` instance: keeps the raising frame's
    /// globals and bound local names, which CPython's traceback reads from
    /// the last frame for the suggestion (the first raise only).
    pub(crate) fn note_raised_name_error(
        &mut self,
        e: Value,
        globals: Value,
        locals: &[String],
    ) -> Result<(), Raised> {
        let mut p = self.hint_data(e, PENDING_NAME).unwrap_or_else(|| {
            vec![
                Value::int(PENDING_NAME).unwrap_or(Value::NONE),
                Value::NONE,
                Value::NONE,
                Value::NONE,
                Value::NONE,
            ]
        });
        p[2] = globals;
        if !locals.is_empty() {
            p[3] = self.new_str(&locals.join("\n"))?;
        }
        let t = self.alloc_tuple(&p)?;
        if let Some(r) = e.as_obj() {
            self.set_slot_o(r, HINT, t);
        }
        Ok(())
    }

    /// The hint text of `exc`'s report, computed now and charged to fuel
    /// (one unit per candidate name), as CPython's traceback computes it:
    /// a `NameError`'s only with a traceback (`with_tb`), from the name and
    /// the raising frame; an `AttributeError`'s from its `name` and `obj`;
    /// an `ImportError`'s from the module.
    pub(crate) fn hint_of(&mut self, exc: Value, with_tb: bool) -> Value {
        let h = self.vslot_o(exc, HINT);
        if !h.is_obj_kind(ty::TUPLE as u8) {
            return h;
        }
        let p = self.tuple_items(h);
        if p.len() != 5 {
            return Value::NONE;
        }
        let (name, data, locals, note) = (p[1], p[2], p[3], p[4]);
        if Vm::is_str(note) {
            return note;
        }
        if !Vm::is_str(name) {
            return Value::NONE;
        }
        let saved = self.exc;
        let text = match p[0].as_int() {
            Some(PENDING_NAME) => {
                // The suggestion needs the raising frame; the stdlib note
                // does not.
                let name = String::from(self.str_ref_o(name));
                let frame = (with_tb && data != Value::NONE).then_some((data, locals));
                self.name_hint_text(&name, frame)
            }
            Some(PENDING_ATTR) if data.is_obj_kind(ty::MODULE as u8) => {
                self.module_attr_hint_text(data, name)
            }
            Some(PENDING_ATTR) => self.attr_hint_text(data, name),
            Some(PENDING_MODULE) => self.module_attr_hint_text(data, name),
            _ => None,
        };
        let v = text
            .and_then(|t| self.new_str(&t).ok())
            .unwrap_or(Value::NONE);
        self.exc = saved;
        v
    }

    /// Records the hint data of a missing module attribute: an
    /// `AttributeError` (`import`: false) or `from m import name`'s
    /// `ImportError`; `note` replaces the suggestion.
    pub(crate) fn set_module_hint_pending(
        &mut self,
        m: Value,
        name: Value,
        import: bool,
        note: Value,
    ) {
        let kind = if import { PENDING_MODULE } else { PENDING_ATTR };
        self.set_hint_pending(kind, name, m, Value::NONE, note);
    }

    /// [`suggest`], charged one fuel unit per candidate.
    pub(crate) fn suggest_charged<'a>(
        &mut self,
        cands: &[&'a str],
        wrong: &str,
    ) -> Option<&'a str> {
        self.burn(cands.len() as u64);
        suggest(cands, wrong)
    }

    /// The names among `names` a "Did you mean" for `wrong` considers
    /// (underscored ones only for an underscored `wrong`), or `None` past
    /// `MAX_CANDIDATES`, counted before any is copied. Charges fuel by
    /// count. Runs only when a pending hint is rendered.
    pub(crate) fn hint_names(&mut self, names: &[Value], wrong: &str) -> Option<Vec<String>> {
        self.burn(names.len() as u64 / 8);
        let keep = |n: &str| wrong.starts_with('_') || !n.starts_with('_');
        let n = names
            .iter()
            .filter(|&&n| Vm::is_str(n) && keep(self.str_ref_o(n)))
            .count();
        if n > MAX_CANDIDATES {
            return None;
        }
        Some(
            names
                .iter()
                .filter(|&&n| Vm::is_str(n) && keep(self.str_ref_o(n)))
                .map(|&n| String::from(self.str_ref_o(n)))
                .collect(),
        )
    }

    /// Records the "Did you mean" hint of an `AttributeError` on `obj.name`
    /// (pending until rendered).
    pub(crate) fn set_attr_hint(&mut self, obj: Value, name: Value) {
        self.set_hint_pending(PENDING_ATTR, name, obj, Value::NONE, Value::NONE);
    }

    fn attr_hint_text(&mut self, obj: Value, name: Value) -> Option<String> {
        let wrong = String::from(self.str_ref_o(name));
        // User instances and classes: `dir()` is the instance's attributes
        // and the namespaces along the MRO.
        let class = if Vm::is_class(obj) {
            Some(obj)
        } else {
            self.user_class(obj)
        };
        if let Some(c) = class {
            let mut names: Vec<Value> = if Vm::is_class(obj) {
                Vec::new()
            } else {
                self.inst_attr_names(obj)
            };
            for k in self.mro_of(c) {
                if Vm::is_class(k) {
                    let d = self.vslot_o(k, crate::class::cls::DICT);
                    names.extend(self.dict_items_vec(d).into_iter().map(|(n, _)| n));
                }
            }
            let owned = self.hint_names(&names, &wrong)?;
            // `dir(obj)`, sorted (the first of equally near names wins).
            let mut cands: Vec<&str> = owned.iter().map(String::as_str).collect();
            crate::ops::heap_sort_by(&mut cands, |a, b| a < b);
            cands.dedup();
            return self
                .suggest_charged(&cands, &wrong)
                .map(|s| format!(". Did you mean: '{s}'?"));
        }
        let owner = self.method_owner(obj);
        let mut cands: Vec<&str> = crate::builtins::method_names(owner);
        crate::ops::heap_sort_by(&mut cands, |a, b| a < b);
        cands.dedup();
        if !wrong.starts_with('_') {
            cands.retain(|c| !c.starts_with('_'));
        }
        self.suggest_charged(&cands, &wrong)
            .map(|s| format!(". Did you mean: '{s}'?"))
    }

    /// Records the hint of a `NameError` for `name` (pending until
    /// rendered); `locals` are the bound local names of the frame.
    pub(crate) fn set_name_hint(&mut self, name: Value, locals: &[String]) {
        let n = self.str_ref_o(name);
        // Outside the subset (docs/spec.md §4): say so where a model reads
        // it, with or without a traceback.
        let note = if matches!(n, "eval" | "exec" | "compile") {
            let hint = format!(". micro-monty does not support {n}()");
            match self.new_str(&hint) {
                Ok(v) => v,
                Err(_) => return,
            }
        } else {
            Value::NONE
        };
        // The frame is gone by the time the report renders: keep its bound
        // names, one per line.
        let locals = if locals.is_empty() || note != Value::NONE {
            Value::NONE
        } else {
            match self.new_str(&locals.join("\n")) {
                Ok(v) => v,
                Err(_) => return,
            }
        };
        self.set_hint_pending(PENDING_NAME, name, self.globals, locals, note);
    }

    /// A `NameError`'s hint: the suggestion from the raising frame's
    /// `(globals, locals)`, when given, then the stdlib module note.
    fn name_hint_text(&mut self, name: &str, frame: Option<(Value, Value)>) -> Option<String> {
        let mut hint = String::new();
        let mut s = None;
        if let Some((globals, locals)) = frame {
            if Vm::is_str(locals) && self.str_ref_o(locals) == SELF_HAS_NAME {
                let _ = write!(hint, ". Did you mean: 'self.{name}'?");
                s = Some(());
            } else {
                let mut owned: Vec<String> = if Vm::is_str(locals) {
                    self.str_ref_o(locals)
                        .split('\n')
                        .map(String::from)
                        .collect()
                } else {
                    Vec::new()
                };
                for (k, _) in self.dict_items_vec(globals) {
                    owned.push(self.str_ref_o(k).to_string());
                }
                for (k, _) in self.dict_items_vec(self.builtins) {
                    owned.push(self.str_ref_o(k).to_string());
                }
                for n in crate::prelude::AS_BUILTINS {
                    if !owned.iter().any(|o| o == n) {
                        owned.push(n.to_string());
                    }
                }
                let cands: Vec<&str> = owned.iter().map(String::as_str).collect();
                if let Some(c) = self.suggest_charged(&cands, name) {
                    let _ = write!(hint, ". Did you mean: '{c}'?");
                    s = Some(());
                }
            }
        }
        if STDLIB.contains(&name) {
            if s.is_some() {
                let _ = write!(hint, " Or did you forget to import '{name}'?");
            } else {
                let _ = write!(hint, ". Did you forget to import '{name}'?");
            }
        }
        (!hint.is_empty()).then_some(hint)
    }

    /// `(type name, str(exc))`.
    pub fn exception_parts(&mut self, exc: Value) -> (String, String) {
        let c = self.vslot_o(exc, exc_slot::TYPE);
        let name = self.class_report_name(c);
        let saved = self.exc;
        self.host_text = true;
        let msg = if let Ok(m) = self.exc_str(exc) {
            m.text
        } else {
            self.slow = false;
            String::from("<exception str() failed>")
        };
        self.host_text = false;
        self.exc = saved;
        (name, msg)
    }

    fn final_line(&mut self, exc: Value) -> String {
        let with_tb = !self.tb_entries(exc).is_empty();
        let mut line = if self.exc_type(exc).is_subclass(T::SyntaxError) {
            let c = self.vslot_o(exc, exc_slot::TYPE);
            let t = self.class_report_name(c);
            self.syntax_lines(exc, &t)
        } else {
            let (t, msg) = self.exception_parts(exc);
            let hint = self.hint_of(exc, with_tb);
            let mut line = if msg.is_empty() {
                t
            } else {
                format!("{t}: {msg}")
            };
            if Vm::is_str(hint) {
                line.push_str(self.str_ref_o(hint));
            }
            line
        };
        line.push('\n');
        // `add_note`'s notes, one line each (CPython's
        // `format_exception_only`).
        if let Some(k) = self.find_interned("__notes__")
            && let Some(notes) = self.inst_get(exc, k)
            && let Some(notes) = self.seq_items(notes)
        {
            for n in notes.into_iter().filter(|&n| Vm::is_str(n)) {
                for l in self.str_ref_o(n).split('\n') {
                    line.push_str(l);
                    line.push('\n');
                }
            }
        }
        line
    }

    /// A `SyntaxError`'s last lines as CPython's traceback prints them
    /// (`_format_syntax_error`): `File "f", line N`, the text and its
    /// carets, then `T: msg` (without the final newline).
    fn syntax_lines(&mut self, exc: Value, t: &str) -> String {
        let mut out = String::new();
        let [file, line, offset, text, end_line, end_offset] = [
            "filename",
            "lineno",
            "offset",
            "text",
            "end_lineno",
            "end_offset",
        ]
        .map(|a| self.syntax_attr(exc, a));
        let mut suffix = String::new();
        if line != Value::NONE {
            let f = if self.truthy(file) {
                self.stringify(file).map(|s| s.text).unwrap_or_default()
            } else {
                String::from("<string>")
            };
            let l = self.stringify(line).map(|s| s.text).unwrap_or_default();
            let _ = writeln!(out, "  File \"{f}\", line {l}");
        } else if file != Value::NONE {
            let f = self.stringify(file).map(|s| s.text).unwrap_or_default();
            suffix = format!(" ({f})");
        }
        let mut cut = false;
        if Vm::is_str(text) {
            // Char counts, without a char copy of a text that can be as
            // long as the memory limit allows.
            let full = self.str_ref_o(text);
            let body = full.trim_end_matches('\n');
            let ltext = body.trim_start_matches([' ', '\n', '\x0c']);
            let rtext = body.chars().count();
            let lstart = rtext - ltext.chars().count();
            // In a host report, only what the host sees is copied.
            let room = crate::scratch::report_len(self, out.len(), ltext);
            cut = room < ltext.len();
            let ltext = &ltext[..room];
            if offset == Value::NONE {
                let _ = writeln!(out, "    {ltext}");
            } else if let Some(mut off) = offset.as_int() {
                let len = full.chars().count() as i64;
                let rlen = rtext as i64;
                let mut end = if line == end_line {
                    end_offset.as_int().filter(|&e| e != 0).unwrap_or(off)
                } else {
                    rlen + 1
                };
                if len > 0 && off > len {
                    off = rlen + 1;
                }
                if len > 0 && end > len {
                    end = rlen + 1;
                }
                if off >= end || end < 0 {
                    end = off + 1;
                }
                let spaces = lstart as i64;
                let (col, end_col) = (off - 1 - spaces, end - 1 - spaces);
                let _ = writeln!(out, "    {ltext}");
                if col >= 0 {
                    let pad: String = ltext
                        .chars()
                        .take(usize::try_from(col).unwrap_or(0))
                        .map(|c| if c.is_whitespace() { c } else { ' ' })
                        .collect();
                    let mut n = usize::try_from(end_col - col).unwrap_or(0);
                    // An empty text leaves the guest's end offset unclamped:
                    // carets that cannot fit are left out, as CPython's
                    // fallback printer does.
                    if self.heap.would_fit(n as u64) {
                        // In a host report, only what the host sees.
                        if n > self.report_cap {
                            n = self.report_cap;
                            cut = true;
                        }
                        out.push_str("    ");
                        out.push_str(&pad);
                        out.extend(core::iter::repeat_n('^', n));
                        out.push('\n');
                    }
                }
            }
        }
        self.report_cut |= cut;
        let msg = self.syntax_attr(exc, "msg");
        let msg = if self.truthy(msg) {
            self.stringify(msg).map(|s| s.text).unwrap_or_default()
        } else {
            String::from("<no detail available>")
        };
        let _ = write!(out, "{t}: {msg}{suffix}");
        out
    }

    /// `exc`'s traceback entries `(code, pc)`, innermost first.
    fn tb_entries(&self, exc: Value) -> Vec<(u32, u32)> {
        let tb = self.vslot_o(exc, exc_slot::TRACEBACK);
        if !tb.is_obj_kind(ty::LIST as u8) {
            return Vec::new();
        }
        self.list_items(tb)
            .into_iter()
            .filter_map(Value::as_int)
            .map(|i| {
                (
                    u32::try_from(i >> 28).unwrap_or(0),
                    u32::try_from(i & 0x0FFF_FFFF).unwrap_or(0),
                )
            })
            .collect()
    }

    /// A traceback object for entry `k` of `exc`'s traceback list.
    fn new_traceback(&mut self, exc: Value, k: usize) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.alloc_init(ty::TRACEBACK, 0, &[exc], &[k as u64]);
            self.hx_o(res)
        }?;
        Ok(Value::from_ref(r, ty::TRACEBACK as u8))
    }

    /// `exc.__traceback__`: the outermost entry, or `None` before it is
    /// raised.
    pub(crate) fn traceback_of(&mut self, exc: Value) -> Result<Value, Raised> {
        match self.tb_entries(exc).len() {
            0 => Ok(Value::NONE),
            n => self.new_traceback(exc, n - 1),
        }
    }

    /// The exception and entry index of traceback object `tb`.
    fn traceback_parts(&self, tb: Value) -> Option<(Value, usize)> {
        let r = tb
            .as_obj()
            .filter(|_| tb.is_obj_kind(ty::TRACEBACK as u8))?;
        Some((
            self.slot_o(r, 0),
            usize::try_from(self.raw_o(r, 1)).unwrap_or(usize::MAX),
        ))
    }

    /// `exc.__traceback__ = v`: `None` empties the traceback; a traceback
    /// object makes it the entries from that one inward.
    pub(crate) fn set_traceback(&mut self, exc: Value, v: Value) -> Result<(), Raised> {
        let Some(r) = exc.as_obj() else { return Ok(()) };
        if v == Value::NONE {
            self.set_slot_o(r, exc_slot::TRACEBACK, Value::NONE);
            return Ok(());
        }
        let Some((owner, k)) = self.traceback_parts(v) else {
            return Err(self.err(T::TypeError, "__traceback__ must be a traceback or None"));
        };
        let tb = self.vslot_o(owner, exc_slot::TRACEBACK);
        let mut items = self.list_items(tb);
        items.truncate(k.saturating_add(1));
        self.burn(items.len() as u64);
        if owner == exc && tb.is_obj_kind(ty::LIST as u8) {
            self.list_set_items(tb, &items)
        } else {
            let l = self.new_list(&items)?;
            self.set_slot_o(r, exc_slot::TRACEBACK, l);
            Ok(())
        }
    }

    /// A traceback object's attributes.
    pub(crate) fn traceback_attr(
        &mut self,
        tb: Value,
        name: &str,
    ) -> Result<Option<Value>, Raised> {
        let Some((exc, k)) = self.traceback_parts(tb) else {
            return Ok(None);
        };
        let entry = self.tb_entries(exc).get(k).copied();
        Ok(match name {
            "tb_next" if k > 0 && entry.is_some() => Some(self.new_traceback(exc, k - 1)?),
            "tb_next" => Some(Value::NONE),
            "tb_lineno" => {
                let line = entry.map_or(0, |(ci, pc)| self.frame_key(ci, pc).1);
                Some(self.new_int(i64::from(line))?)
            }
            _ => None,
        })
    }

    /// `traceback._extract(tb, limit)`: `(filename, lineno, name, line,
    /// text)` for each frame from `tb` inward that has source (bundled
    /// frames never show), `text` its formatted entry; `limit` as
    /// `traceback.extract_tb`'s.
    pub(crate) fn tb_extract(&mut self, tb: Value, limit: Value) -> Result<Value, Raised> {
        let mut frames = Vec::new();
        if let Some((exc, k)) = self.traceback_parts(tb) {
            let entries = self.tb_entries(exc);
            self.burn(entries.len() as u64);
            let mut lines = LineCache::new();
            for &(ci, pc) in entries.iter().take(k.saturating_add(1)).rev() {
                let text = self.format_frame(ci, pc, &mut lines);
                if text.is_empty() {
                    continue;
                }
                let (file, line, name) = self.frame_key(ci, pc);
                let src = self
                    .source_line(ci, line, &mut lines)
                    .map(|l| String::from(l.trim()));
                frames.push((file, line, name, src, text));
            }
        }
        if limit != Value::NONE {
            let Some(n) = self.int_of_o(limit) else {
                return Err(self.err(T::TypeError, "limit must be an integer or None"));
            };
            let n = usize::try_from(n.unsigned_abs()).unwrap_or(usize::MAX);
            if limit.as_int().is_some_and(|l| l >= 0) {
                frames.truncate(n);
            } else {
                frames.drain(..frames.len().saturating_sub(n));
            }
        }
        let mut out = Vec::with_capacity(frames.len());
        for (file, line, name, src, text) in frames {
            let parts = [
                self.new_str(&file)?,
                self.new_int(i64::from(line))?,
                self.new_str(&name)?,
                match src {
                    Some(s) => self.new_str(&s)?,
                    None => Value::NONE,
                },
                self.new_str(&text)?,
            ];
            out.push(self.alloc_tuple(&parts)?);
        }
        self.new_list(&out)
    }

    /// The full CPython 3.14 report of an uncaught exception, chained
    /// causes and contexts included, exception groups drawn as
    /// `TracebackException.format` draws them.
    pub fn render_exception(&mut self, exc: Value) -> String {
        let mut out = String::new();
        let mut need_close = false;
        self.render_into(exc, 0, &mut need_close, &mut out);
        out
    }

    /// The sub-exceptions of an `ExceptionGroup` (the prelude class keeps
    /// them in `_mm_excs`), `None` for other exceptions.
    fn group_items(&self, e: Value) -> Option<Vec<Value>> {
        let k = self.find_interned("_mm_excs")?;
        let t = self.inst_get(e, k)?;
        t.is_obj_kind(ty::TUPLE as u8).then(|| self.tuple_items(t))
    }

    /// `exc`'s chain at exception-group depth `depth`. The recursion into
    /// sub-exceptions stops at CPython's `max_group_depth` (10), so it is
    /// bounded.
    fn render_into(&mut self, exc: Value, depth: usize, need_close: &mut bool, out: &mut String) {
        const MAX_WIDTH: usize = 15;
        const MAX_DEPTH: usize = 10;
        let emit = |out: &mut String, text: &str, d: usize, margin: char| {
            for line in text.split_inclusive('\n') {
                for _ in 0..2 * d {
                    out.push(' ');
                }
                if d > 0 {
                    out.push(margin);
                    out.push(' ');
                }
                out.push_str(line);
            }
        };
        let mut chain: Vec<(Option<&str>, Value)> = Vec::new();
        let mut seen = NativeIds::default();
        let mut e = exc;
        // Out of memory, the chain stops there, as at a cycle.
        let _ = seen.insert(&mut self.heap, e);
        while e.is_obj_kind(ty::EXC as u8) {
            let cause = self.vslot_o(e, exc_slot::CAUSE);
            let context = self.vslot_o(e, exc_slot::CONTEXT);
            let suppress = e
                .as_obj()
                .is_some_and(|r| self.raw_o(r, exc_slot::FLAGS) & 1 != 0);
            let (link, next) = if cause.is_obj_kind(ty::EXC as u8) {
                (
                    "\nThe above exception was the direct cause of the following exception:\n\n",
                    cause,
                )
            } else if context.is_obj_kind(ty::EXC as u8) && !suppress {
                (
                    "\nDuring handling of the above exception, another exception occurred:\n\n",
                    context,
                )
            } else {
                ("", Value::NONE)
            };
            let ok = next.is_obj_kind(ty::EXC as u8)
                && seen.insert(&mut self.heap, next).unwrap_or(false);
            chain.push((if ok { Some(link) } else { None }, e));
            if !ok {
                break;
            }
            e = next;
        }
        for i in (0..chain.len()).rev() {
            // A report stops at its cap or, past 64 exceptions, at the
            // deadline: shared groups can draw 15^10 of them.
            self.report_steps = self.report_steps.saturating_add(1);
            if self.report_stop(out.len(), self.report_steps > 64) {
                return;
            }
            let (m, ex) = chain[i];
            if let Some(m) = m {
                emit(out, m, depth, '|');
            }
            let Some(items) = self.group_items(ex) else {
                let tb = self.format_tb(ex);
                emit(out, &tb, depth, '|');
                let last = self.final_line(ex);
                emit(out, &last, depth, '|');
                continue;
            };
            if depth > MAX_DEPTH {
                emit(out, "... (max_group_depth is 10)\n", depth, '|');
                continue;
            }
            let top = depth == 0;
            let d = if top { 1 } else { depth };
            let tb = self.format_tb(ex);
            if let Some(rest) = tb.strip_prefix("Traceback (most recent call last):\n") {
                emit(
                    out,
                    "Exception Group Traceback (most recent call last):\n",
                    d,
                    if top { '+' } else { '|' },
                );
                emit(out, rest, d, '|');
            }
            let last = self.final_line(ex);
            emit(out, &last, d, '|');
            let n = items.len().min(MAX_WIDTH + 1);
            *need_close = false;
            for (j, &sub) in items.iter().take(n).enumerate() {
                let last_exc = j + 1 == n;
                if last_exc {
                    *need_close = true;
                }
                for _ in 0..2 * d {
                    out.push(' ');
                }
                out.push_str(if j == 0 { "+-" } else { "  " });
                if j >= MAX_WIDTH {
                    out.push_str("+---------------- ... ----------------\n");
                    let more = items.len() - MAX_WIDTH;
                    let text = format!(
                        "and {more} more exception{}\n",
                        if more > 1 { "s" } else { "" }
                    );
                    emit(out, &text, d + 1, '|');
                } else {
                    let _ = writeln!(out, "+---------------- {} ----------------", j + 1);
                    self.render_into(sub, d + 1, need_close, out);
                }
                if last_exc && *need_close {
                    for _ in 0..2 * (d + 1) {
                        out.push(' ');
                    }
                    out.push_str("+------------------------------------\n");
                    *need_close = false;
                }
            }
        }
    }

    fn format_tb(&mut self, exc: Value) -> String {
        let mut entries = self.tb_entries(exc);
        if entries.is_empty() {
            return String::new();
        }
        entries.reverse();
        let mut out = String::from("Traceback (most recent call last):\n");
        let mut last: Option<(u32, u32, String)> = None;
        let mut count = 0usize;
        let mut lines = LineCache::new();
        for (ci, pc) in entries {
            let (file, line, name) = self.frame_key(ci, pc);
            let key = (
                self.codes.get(ci as usize).map_or(0, |c| c.source),
                line,
                name.clone(),
            );
            if last
                .as_ref()
                .is_none_or(|(s, l, n)| (*s, *l, n.as_str()) != (key.0, key.1, key.2.as_str()))
            {
                if count > RECURSIVE_CUTOFF {
                    let c = count - RECURSIVE_CUTOFF;
                    let _ = writeln!(
                        out,
                        "  [Previous line repeated {c} more time{}]",
                        if c > 1 { "s" } else { "" }
                    );
                }
                last = Some(key);
                count = 0;
            }
            count += 1;
            if count > RECURSIVE_CUTOFF {
                continue;
            }
            let _ = file;
            out.push_str(&self.format_frame(ci, pc, &mut lines));
        }
        if count > RECURSIVE_CUTOFF {
            let c = count - RECURSIVE_CUTOFF;
            let _ = writeln!(
                out,
                "  [Previous line repeated {c} more time{}]",
                if c > 1 { "s" } else { "" }
            );
        }
        out
    }

    fn frame_key(&self, ci: u32, pc: u32) -> (String, u32, String) {
        let Some(c) = self.codes.get(ci as usize) else {
            return (String::new(), 0, String::new());
        };
        let line = c.code.position(pc).map_or(0, |p| p.pos.line);
        let file = self
            .source_of(ci)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        (file, line, String::from(&*c.code.name))
    }

    /// One frame of a traceback: header, source lines and carets.
    /// The byte ranges of the lines of code `ci`'s source, split once per
    /// traceback and source (once per frame was O(frames × source)).
    fn line_ranges<'c>(&self, ci: u32, cache: &'c mut LineCache) -> &'c [(usize, usize)] {
        let Some(src) = self.codes.get(ci as usize).map(|c| c.source) else {
            return &[];
        };
        let i = if let Some(i) = cache.iter().position(|(k, _)| *k == src) {
            i
        } else {
            let text = self
                .sources
                .get(src as usize)
                .map_or("", |s| s.text.as_str());
            let base = text.as_ptr() as usize;
            let ranges = mm_compile::source_lines(text)
                .map(|l| {
                    (
                        l.as_ptr() as usize - base,
                        l.as_ptr() as usize - base + l.len(),
                    )
                })
                .collect();
            cache.push((src, ranges));
            cache.len() - 1
        };
        &cache[i].1
    }

    /// 1-based line `line` of code `ci`'s source.
    fn source_line(&self, ci: u32, line: u32, cache: &mut LineCache) -> Option<&str> {
        let &(a, b) = self
            .line_ranges(ci, cache)
            .get((line as usize).checked_sub(1)?)?;
        self.source_of(ci)?.text.get(a..b)
    }

    #[allow(clippy::too_many_lines, reason = "a port of format_frame_summary")]
    fn format_frame(&self, ci: u32, pc: u32, lines: &mut LineCache) -> String {
        let Some(c) = self.codes.get(ci as usize) else {
            return String::new();
        };
        let Some(src) = self.source_of(ci) else {
            return String::new();
        };
        let pe = c.code.position(pc).copied();
        let line = pe.map_or(0, |p| p.pos.line);
        let mut row = format!(
            "  File \"{}\", line {}, in {}\n",
            src.name, line, c.code.name
        );
        let Some(pe) = pe else { return row };
        if line == 0 {
            return row;
        }
        let p = pe.pos;
        let nlines = self.line_ranges(ci, lines).len();
        let end_line = p.end_line.max(p.line);
        // A range longer than the source (restored code is not trusted)
        // shows no source line.
        if (end_line - p.line) as usize > nlines {
            return row;
        }
        let mut orig: Vec<String> = Vec::new();
        for l in p.line..=end_line {
            let t = self.source_line(ci, l, lines).unwrap_or("");
            orig.push(t.trim_end().to_string());
        }
        let ded = dedent(&orig);
        if ded.iter().all(|l| l.trim().is_empty()) {
            return row;
        }
        let first = &orig[0];
        let all = &ded;
        let dedent0 = char_len(first) - char_len(&all[0]);
        let start = (p.col as usize).saturating_sub(dedent0);
        let end_dedent = char_len(&orig[orig.len() - 1]) - char_len(&all[all.len() - 1]);
        let end = (p.end_col as usize).saturating_sub(end_dedent);
        let dp_start = display_width(&all[0], start);
        let dp_end = display_width(&all[all.len() - 1], end);
        let a = pe.anchor;
        let has_anchor = a.kind != 0
            && (p.line..=end_line).contains(&a.left_line)
            && (p.line..=end_line).contains(&a.right_line);
        let (al, ar, al_off, ar_off, primary, secondary) = if has_anchor {
            let li = (a.left_line - p.line) as usize;
            let ri = (a.right_line - p.line) as usize;
            let dl = char_len(&orig[li]) - char_len(&all[li]);
            let dr = char_len(&orig[ri]) - char_len(&all[ri]);
            (
                li,
                ri,
                display_width(&all[li], (a.left_col as usize).saturating_sub(dl)),
                display_width(&all[ri], (a.right_col as usize).saturating_sub(dr)),
                '~',
                '^',
            )
        } else {
            (0, 0, 0, 0, '^', '^')
        };
        let show = if pe.no_carets {
            false
        } else if has_anchor {
            true
        } else {
            let before: String = all[0].chars().take(start).collect();
            let after: String = all[all.len() - 1].chars().skip(end).collect();
            !before.trim_start().is_empty() || !after.trim_end().is_empty()
        };
        let n = all.len();
        let mut marks = vec![0, n - 1];
        if has_anchor {
            for k in [al, ar] {
                marks.extend(k.saturating_sub(1)..=k + 1);
            }
        }
        let sig: Vec<usize> = (0..n).filter(|k| marks.contains(k)).collect();
        let mut result: Vec<String> = Vec::new();
        let output_line = |k: usize, result: &mut Vec<String>| {
            result.push(format!("{}\n", all[k]));
            if !show {
                return;
            }
            let num_spaces = char_len(&all[k]) - char_len(all[k].trim_start());
            let num_carets = if k == n - 1 {
                dp_end
            } else {
                display_width(&all[k], char_len(&all[k]))
            };
            let mut carets = String::with_capacity(num_carets);
            for col in 0..num_carets {
                if col < num_spaces || (k == 0 && col < dp_start) {
                    carets.push(' ');
                } else if has_anchor
                    && (k > al || (k == al && col >= al_off))
                    && (k < ar || (k == ar && col < ar_off))
                {
                    carets.push(secondary);
                } else {
                    carets.push(primary);
                }
            }
            carets.push('\n');
            result.push(carets);
        };
        for (i, &k) in sig.iter().enumerate() {
            if i > 0 {
                let diff = k - sig[i - 1];
                if diff == 2 {
                    output_line(k - 1, &mut result);
                } else if diff > 2 {
                    result.push(format!("...<{} lines>...\n", diff - 1));
                }
            }
            output_line(k, &mut result);
        }
        // textwrap.indent(textwrap.dedent(...), '    ')
        let lines: Vec<String> = result
            .iter()
            .map(|l| l.trim_end_matches('\n').to_string())
            .collect();
        for l in dedent(&lines) {
            row.push_str("    ");
            row.push_str(&l);
            row.push('\n');
        }
        row
    }
}
