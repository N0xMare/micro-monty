//! Exceptions: context chaining, tracebacks as `(code, pc)` pairs, "Did you
//! mean" hints, and CPython 3.14's traceback text with `~~^~~` carets.

use crate::hints::HintGroup;
use crate::scratch::{NativeIds, NativeText};
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
/// A `UnicodeDecodeError` or `UnicodeEncodeError`, which has no hint, keeps
/// its five fields here instead ([`Vm::unicode_init`]).
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
/// `UnicodeDecodeError`'s and `UnicodeEncodeError`'s attributes, in the
/// order of the constructor's arguments.
pub(crate) const UNICODE_ATTRS: [&str; 5] = ["encoding", "object", "start", "end", "reason"];
const RECURSIVE_CUTOFF: usize = 3;
const MAX_CANDIDATES: usize = 750;
const MAX_STRING: usize = 40;
const MOVE_COST: usize = 2;
const CASE_COST: usize = 1;

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
    #[cold]
    #[inline(never)]
    pub(crate) fn not_integer(&mut self, v: Value) -> Raised {
        let msg = format!(
            "'{}' object cannot be interpreted as an integer",
            self.type_name(v)
        );
        self.err(T::TypeError, &msg)
    }

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
        let a = self.alloc_tuple_hot(args)?;
        self.alloc_exc(tv, a)
    }

    /// Sets `exc.__context__` to the exception being handled (CPython's
    /// `_PyErr_SetObject`), breaking any cycle that would create, as `exc`
    /// is raised at `(ci, pc)`; the heap's shared `MemoryError` is raised
    /// as [`Vm::raise_no_memory`] says.
    pub(crate) fn attach_context(&mut self, exc: Value, ci: u32, pc: u32) {
        if exc == self.memory_error && self.raise_no_memory(ci, pc) {
            return;
        }
        let h = self.handled;
        if h == Value::NONE || h == exc || !exc.is_obj_kind(ty::EXC as u8) {
            return;
        }
        let mut o = h;
        for _ in 0..10_000 {
            let c = self.vslot_hot(o, exc_slot::CONTEXT);
            if c == Value::NONE || c == Value::UNDEFINED {
                break;
            }
            if c == exc {
                if let Some(r) = o.as_obj() {
                    self.set_slot_hot(r, exc_slot::CONTEXT, Value::NONE);
                }
                break;
            }
            o = c;
        }
        if let Some(r) = exc.as_obj() {
            self.set_slot_hot(r, exc_slot::CONTEXT, h);
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

    /// Raises the heap's shared `MemoryError` (`memory_error`: a block that
    /// did not fit, or a size the heap refused or that passes a cap) at
    /// `(ci, pc)`: a fresh `MemoryError()` in its place, as CPython's
    /// `PyErr_NoMemory` takes one from its free list, so an error saved by
    /// an `except` keeps its own traceback and attributes (two were one
    /// object, with the second raise's traceback [M, reviewer]). Like
    /// CPython's, it has no `__context__`, also inside an `except` block:
    /// true. With no room for one, the shared one is raised, cleared; caught
    /// and raised again by `raise e`, it stays itself and takes its context
    /// (false). A full arena may be mostly garbage that no successful
    /// allocation would ever schedule a collection for, so one is asked
    /// for. Out of line: `attach_context` is inlined into `Vm::unwind`.
    #[cold]
    #[inline(never)]
    fn raise_no_memory(&mut self, ci: u32, pc: u32) -> bool {
        self.request_gc();
        let by_raise = ci != self.tramp
            && self.codes.get(ci as usize).is_some_and(|c| {
                c.code
                    .bytecode
                    .get(pc as usize)
                    .copied()
                    .map(crate::interp::op_of)
                    == Some(mm_compile::bytecode::op::RAISE)
            });
        if by_raise {
            self.fresh_memory_error();
            return false;
        }
        let tv = self.types[T::MemoryError as usize];
        let args = self.empty_tuple;
        if let Ok(e) = self.alloc_exc(tv, args) {
            self.exc = e;
        } else {
            self.exc = self.memory_error;
            self.fresh_memory_error();
        }
        // Its own failed request, if it had no room, dies with it.
        self.heap.take_refused();
        true
    }

    /// Restores `e` as the exception being raised after work on it that
    /// may have failed for room: a refusal that failure left dies, as it
    /// raised nothing (`Vm::unwind` drops only a raised one's).
    fn keep_raising(&mut self, e: Value) {
        if self.exc != e {
            self.heap.take_refused();
        }
        self.exc = e;
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
        let mut tb = self.slot_hot(r, exc_slot::TRACEBACK);
        if !tb.is_obj_kind(ty::LIST as u8) {
            match self.new_list(&[]) {
                Ok(l) => {
                    tb = l;
                    self.set_slot_hot(r, exc_slot::TRACEBACK, l);
                }
                Err(Raised) => return self.drop_tb_refusal(),
            }
        }
        let entry = (i64::from(ci) << 28) | i64::from(pc & 0x0FFF_FFFF);
        if let Some(v) = Value::int(entry) {
            let saved = self.exc;
            if self.list_push(tb, v).is_err() {
                self.drop_tb_refusal();
            }
            self.exc = saved;
        }
    }

    /// A traceback that did not grow for room raises nothing of its own:
    /// the refusal dies (`Vm::unwind` dropped the raised error's before).
    #[cold]
    #[inline(never)]
    fn drop_tb_refusal(&mut self) {
        self.heap.take_refused();
    }

    pub(crate) fn set_hint_suffix(&mut self, hint: &str) {
        let e = self.exc;
        if e != self.memory_error
            && let Ok(s) = self.new_str(hint)
            && let Some(r) = e.as_obj()
            && e.is_obj_kind(ty::EXC as u8)
        {
            self.set_slot(r, HINT, s);
        }
        self.keep_raising(e);
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
        // Not on the shared `MemoryError` raised when the error itself had
        // no room: it is a root, and its hint would keep `data` (a frame's
        // globals) alive past `reset`, which then freed nothing.
        if let Some(r) = e.as_obj()
            && e != self.memory_error
            && e.is_obj_kind(ty::EXC as u8)
            && let Some(k) = Value::int(kind)
            && let Ok(t) = self.alloc_tuple(&[k, name, data, locals, note])
        {
            self.set_slot(r, HINT, t);
        }
        self.keep_raising(e);
    }

    /// The hint data of `e` (see [`HINT`]), when it is of `kind`.
    fn hint_data(&self, e: Value, kind: i64) -> Option<Vec<Value>> {
        let h = self.vslot(e, HINT);
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
        match self.class_base_t(self.vslot(e, exc_slot::TYPE)) {
            T::NameError | T::UnboundLocalError => Some(PENDING_NAME),
            T::AttributeError => Some(PENDING_ATTR),
            _ => None,
        }
    }

    /// Is `e` an `ImportError` (or a subclass)?
    pub(crate) fn is_import_error(&self, e: Value) -> bool {
        matches!(
            self.class_base_t(self.vslot(e, exc_slot::TYPE)),
            T::ImportError | T::ModuleNotFoundError
        )
    }

    /// An `ImportError`'s `name`, `path`, `name_from` or `msg` when its
    /// instance dict has none: `msg` is the one argument, the rest `None`.
    pub(crate) fn import_attr(&self, e: Value, attr: &str) -> Value {
        let args = self.vslot(e, exc_slot::ARGS);
        if attr == "msg" && self.tuple_len(args) == 1 {
            self.vslot(args, 0)
        } else {
            Value::NONE
        }
    }

    /// Sets `name` (and `name_from`, unless `UNDEFINED`) on the
    /// `ImportError` being raised. Unlike a hint, these are the error's
    /// own attributes: when one has no room the `MemoryError` is raised
    /// instead, as CPython's error would not have been made.
    pub(crate) fn set_import_names(&mut self, name: &str, from: Value) {
        let e = self.exc;
        if e == self.memory_error {
            return;
        }
        let set = |vm: &mut Vm, k: &str, v: Value| -> Result<(), Raised> {
            let k = vm.intern(k)?;
            vm.inst_set(e, k, v).map(drop)
        };
        let _ = self
            .new_str(name)
            .and_then(|n| set(self, "name", n))
            .and_then(|()| {
                if from == Value::UNDEFINED {
                    return Ok(());
                }
                set(self, "name_from", from)
            });
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
        let args = self.vslot(e, exc_slot::ARGS);
        let n = self.tuple_len(args);
        if attr == "msg" {
            return if n == 0 {
                Value::NONE
            } else {
                self.vslot(args, 0)
            };
        }
        let info = self.vslot(args, 1);
        if n != 2 || !info.is_obj_kind(ty::TUPLE as u8) {
            return Value::NONE;
        }
        let len = self.tuple_len(info);
        if !(4..=6).contains(&len) {
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
        if i < len {
            self.vslot(info, i as u32)
        } else {
            Value::NONE
        }
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
            let f = self.str_ref(file);
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

    /// Whether `e` is a `UnicodeDecodeError` (`Some(true)`) or a
    /// `UnicodeEncodeError` (`Some(false)`), or a subclass of one.
    pub(crate) fn unicode_decode(&self, e: Value) -> Option<bool> {
        match self.class_base_t(self.vslot(e, exc_slot::TYPE)) {
            T::UnicodeDecodeError => Some(true),
            T::UnicodeEncodeError => Some(false),
            _ => None,
        }
    }

    /// Whether `str(e)` is built natively from [`Vm::plain_scalar`]
    /// arguments only, so it runs no user code and allocates nothing but
    /// its text (`Vm::call_only_allocates`): a builtin exception other than
    /// a `SyntaxError` or a Unicode error, whose texts read attributes any
    /// value can be set to.
    #[cold]
    #[inline(never)]
    pub(crate) fn exc_str_plain(&self, e: Value) -> bool {
        // A builtin exception's type slot is its type object; a user
        // class's is not one.
        let Some(t) = e
            .is_obj_kind(ty::EXC as u8)
            .then(|| self.type_of_typeobj(self.vslot(e, exc_slot::TYPE)))
            .flatten()
        else {
            return false;
        };
        let args = self.vslot(e, exc_slot::ARGS);
        !matches!(
            t,
            T::SyntaxError
                | T::IndentationError
                | T::TabError
                | T::UnicodeDecodeError
                | T::UnicodeEncodeError
        ) && args.is_obj_kind(ty::TUPLE as u8)
            && (0..self.tuple_len(args)).all(|i| Self::plain_scalar(self.vslot(args, i as u32)))
    }

    /// Field `i` of [`UNICODE_ATTRS`] as a Unicode error keeps it in its
    /// [`HINT`] slot, a tuple of the five that its constructor writes:
    /// `UNDEFINED` where none was set or it was deleted (CPython's `NULL`),
    /// an int for `start` and `end` (0 until set).
    pub(crate) fn unicode_raw(&self, e: Value, i: usize) -> Value {
        let t = self.vslot(e, HINT);
        if t.is_obj_kind(ty::TUPLE as u8) {
            self.vslot(t, i as u32)
        } else if i == 2 || i == 3 {
            Value::int(0).unwrap_or(Value::NONE)
        } else {
            Value::UNDEFINED
        }
    }

    /// Field `i` of [`UNICODE_ATTRS`] of a Unicode error, as read: `None`
    /// where none is set.
    pub(crate) fn unicode_field(&self, e: Value, i: usize) -> Value {
        match self.unicode_raw(e, i) {
            Value::UNDEFINED => Value::NONE,
            v => v,
        }
    }

    /// `v` as CPython's `Py_ssize_t` conversion takes it: the int of an int
    /// (a bool or an int subclass's included), `OverflowError` past it, and
    /// `None` for anything else.
    fn unicode_ssize(&mut self, v: Value) -> Result<Option<Value>, Raised> {
        let v = self.unbx(v);
        match self.int_of(v) {
            Some(i) => self.new_int(i).map(Some),
            None if Vm::is_heap_int(v) => Err(self.err(
                T::OverflowError,
                "Python int too large to convert to C ssize_t",
            )),
            None => Ok(None),
        }
    }

    /// Sets (`Some`) or deletes field `i` of Unicode error `e`, as CPython's
    /// members do: `start` and `end` take an int only and cannot be
    /// deleted; the other three take any value.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn unicode_set_field(&mut self, e: Value, i: usize, v: Option<Value>) -> Result<(), Raised> {
        let v = match (i, v) {
            (2 | 3, None) => {
                return Err(self.err(T::TypeError, "can't delete numeric/char attribute"));
            }
            (2 | 3, Some(v)) => match self.unicode_ssize(v)? {
                Some(n) => n,
                None => return Err(self.err(T::TypeError, "an integer is required")),
            },
            (_, None) => Value::UNDEFINED,
            (_, Some(v)) => v,
        };
        let mut t = self.vslot(e, HINT);
        if !t.is_obj_kind(ty::TUPLE as u8) {
            let zero = Value::int(0).unwrap_or(Value::NONE);
            let unset = Value::UNDEFINED;
            t = self.alloc_tuple(&[unset, unset, zero, zero, unset])?;
            if let Some(r) = e.as_obj() {
                self.set_slot(r, HINT, t);
            }
        }
        if let Some(r) = t.as_obj() {
            self.set_slot(r, i as u32, v);
        }
        Ok(())
    }

    /// `e.name = v` (`Some`) or `del e.name` of field `i` of
    /// [`UNICODE_ATTRS`] of a Unicode error; `false` for other exceptions,
    /// and where a user class defines the name, which hides the member as
    /// in CPython (the instance's dict takes it).
    pub(crate) fn unicode_attr_set(
        &mut self,
        e: Value,
        i: usize,
        v: Option<Value>,
    ) -> Result<bool, Raised> {
        let c = self.class_of(e);
        if self.unicode_decode(e).is_none()
            || (Vm::is_class(c)
                && self
                    .find_interned(UNICODE_ATTRS[i])
                    .is_some_and(|k| self.class_lookup(c, k).is_some()))
        {
            return Ok(false);
        }
        self.unicode_set_field(e, i, v)?;
        Ok(true)
    }

    /// `encoding` or `reason` (field `i`) of a Unicode error for its
    /// `str()`: `UNDEFINED` when unset (shown as `<NULL>`), the `str` it is
    /// or extends without a `__str__` of its own, else `None`: its `str()`
    /// may run Python code or be another exception's, which may lead back
    /// here, so the prelude's `_str` takes it.
    pub(crate) fn unicode_text(&self, e: Value, i: usize) -> Option<Value> {
        match self.unicode_raw(e, i) {
            Value::UNDEFINED => Some(Value::UNDEFINED),
            v => {
                let v = self.str_backing(v).unwrap_or(v);
                Vm::is_str(v).then_some(v)
            }
        }
    }

    /// `reason` and `encoding` when `str()` takes them as they are: both
    /// `str`s (or unset, `<NULL>`) of at most 1 MiB together. Else `str()`
    /// and an uncaught error's report ([`Vm::exc_needs_py`]) go through
    /// the prelude, which joins them a step at a time. Out of line: inlined
    /// in both callers it was 80 B more native code [M].
    #[inline(never)]
    pub(crate) fn unicode_texts(&self, e: Value) -> Option<(Value, Value)> {
        let (r, c) = (self.unicode_text(e, 4)?, self.unicode_text(e, 0)?);
        let len = |v: Value| {
            if Vm::is_str(v) {
                self.str_bytes(v).len()
            } else {
                0
            }
        };
        (len(r) + len(c) <= 1 << 20).then_some((r, c))
    }

    /// `str()` of a `UnicodeDecodeError` or `UnicodeEncodeError`, from its
    /// fields as CPython's: empty without an `object` (an instance its
    /// constructor never saw); one byte or character named when
    /// `start..end` is one inside `object`, else the range. `whole`: with
    /// `encoding` and `reason` as they are, when both are `str`s (or unset,
    /// `<NULL>`) of at most a piece together, else `need_py`; without, the
    /// middle alone, which the prelude's `_str` joins a step at a time with
    /// their `str()`, taken first as CPython takes them, before `object` is
    /// checked. `None` for other exceptions.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn unicode_str(
        &mut self,
        e: Value,
        whole: bool,
    ) -> Result<Option<NativeText>, Raised> {
        let Some(decode) = self.unicode_decode(e) else {
            return Ok(None);
        };
        let mut s = NativeText::default();
        let object = self.unicode_raw(e, 1);
        if object == Value::UNDEFINED {
            return Ok(Some(s));
        }
        let mut texts = None;
        if whole {
            texts = match self.unicode_texts(e) {
                Some(t) => Some(t),
                None => return Err(self.need_py()),
            };
        }
        let object = self.unbx(object);
        let len = if decode && Vm::is_bytes(object) {
            self.bytes_data(object).len()
        } else if !decode && Vm::is_str(object) {
            self.str_len(object)
        } else {
            let msg = if decode {
                "UnicodeError 'object' attribute must be a bytes"
            } else {
                "UnicodeError 'object' attribute must be a string"
            };
            return Err(self.err(T::TypeError, msg));
        };
        let field = |vm: &Vm, i: usize| vm.int_of(vm.unicode_raw(e, i)).unwrap_or(0);
        let (start, end) = (field(self, 2), field(self, 3));
        if let Some((_, encoding)) = texts {
            s.push(self, "'")?;
            self.unicode_push(&mut s, encoding)?;
        }
        let one = usize::try_from(start)
            .ok()
            .filter(|&i| i < len && start.checked_add(1) == Some(end));
        let what = match one {
            Some(i) if decode => {
                let byte = self.bytes_data(object).get(i).copied().unwrap_or(0);
                format!("' codec can't decode byte 0x{byte:02x} in position {start}: ")
            }
            Some(i) => {
                // Read from the bytes: `str_ref_o` would validate all of
                // `object` for one char.
                let at = self.str_offset(object, i);
                let ch = crate::strmeth::char_at(self.str_bytes(object), at).map_or('\0', |c| c.0);
                format!(
                    "' codec can't encode character '{}' in position {start}: ",
                    crate::bytes::escaped_char(ch)
                )
            }
            None => format!(
                "' codec can't {} in position {start}-{}: ",
                if decode {
                    "decode bytes"
                } else {
                    "encode characters"
                },
                end.wrapping_sub(1)
            ),
        };
        s.push(self, &what)?;
        if let Some((reason, _)) = texts {
            self.unicode_push(&mut s, reason)?;
        }
        Ok(Some(s))
    }

    /// `encoding` or `reason` as [`Vm::unicode_text`] gave it, into `s`.
    fn unicode_push(&mut self, s: &mut NativeText, v: Value) -> Result<(), Raised> {
        if v == Value::UNDEFINED {
            s.push(self, "<NULL>")
        } else {
            s.push_value(self, v)
        }
    }

    /// CPython's `UnicodeDecodeError.__init__` (`decode`) or
    /// `UnicodeEncodeError.__init__` on arguments `a`, a tuple (`e`'s
    /// `args`, or what the prelude's `_unicode_init` made of them): five,
    /// checked in its order (a decode's `object` last), kept as `e`'s
    /// fields. `Some(i)` when argument `i` needs Python code first, and
    /// nothing is kept: a `start` or `end` with its own `__index__`, or a
    /// decode's bytes-like `object` that is not a `bytes` (its copy is the
    /// field, made by `bytes()` a step at a time). Out of line: the call
    /// paths that reach it stay as they were for every other type.
    #[cold]
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn unicode_init(
        &mut self,
        e: Value,
        decode: bool,
        a: Value,
    ) -> Result<Option<usize>, Raised> {
        let n = self.tuple_len(a);
        if n != 5 {
            let msg = format!("function takes exactly 5 arguments ({n} given)");
            return Err(self.err(T::TypeError, &msg));
        }
        let order = if decode {
            [0, 2, 3, 4, 1]
        } else {
            [0, 1, 2, 3, 4]
        };
        let mut fields = [Value::NONE; 5];
        for i in order {
            let v = self.vslot(a, i as u32);
            let b = self.unbx(v);
            fields[i] = match i {
                2 | 3 => match self.unicode_ssize(v)? {
                    Some(n) => n,
                    None if self.uflags(v) & crate::class::f::INDEX != 0 => return Ok(Some(i)),
                    None => return Err(self.not_integer(v)),
                },
                1 if decode && Vm::is_bytes(b) => v,
                1 if decode && Vm::is_bytearray(b) => return Ok(Some(1)),
                1 if decode => return self.need_bytes(v).map(|_| None),
                _ if Vm::is_str(b) => v,
                _ => {
                    let tn = if v == Value::NONE {
                        "None"
                    } else {
                        self.type_name(v)
                    };
                    let msg = format!("argument {} must be str, not {tn}", i + 1);
                    return Err(self.err(T::TypeError, &msg));
                }
            };
        }
        let t = self.alloc_tuple(&fields)?;
        if let Some(r) = e.as_obj() {
            self.set_slot(r, HINT, t);
        }
        Ok(None)
    }

    /// Unicode error `e` made from its `args`, its fields kept
    /// ([`Vm::unicode_init`]); one that needs Python code first goes on in
    /// the prelude's `_unicode_init`, which gives `e` back.
    #[cold]
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn unicode_new(&mut self, e: Value, decode: bool) -> Result<Value, Raised> {
        let a = self.vslot(e, exc_slot::ARGS);
        if self.unicode_init(e, decode, a)?.is_none() {
            return Ok(e);
        }
        let d = Value::from_bool(decode);
        self.prelude_request("_unicode_init", vec![e, d, a, e], crate::cont::Cont::None)
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
            self.set_slot(r, HINT, t);
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
                if !IMPORT_ATTRS.contains(&self.str_ref(k)) {
                    let msg = format!(
                        "ImportError() got an unexpected keyword argument '{}'",
                        self.str_ref(k)
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
            let n = String::from(self.str_ref(k));
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
            self.set_slot(r, HINT, t);
        }
        Ok(())
    }

    /// The hint text of `exc`'s report, computed now and charged to fuel
    /// (one unit per candidate name), as CPython's traceback computes it:
    /// a `NameError`'s only with a traceback (`with_tb`), from the name and
    /// the raising frame; an `AttributeError`'s from its `name` and `obj`;
    /// an `ImportError`'s from the module.
    pub(crate) fn hint_of(&mut self, exc: Value, with_tb: bool) -> Value {
        let h = self.vslot(exc, HINT);
        // A Unicode error's tuple there is its fields.
        if !h.is_obj_kind(ty::TUPLE as u8) || self.unicode_decode(exc).is_some() {
            return if Vm::is_str(h) { h } else { Value::NONE };
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
                let name = String::from(self.str_ref(name));
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
            .filter(|&&n| Vm::is_str(n) && keep(self.str_ref(n)))
            .count();
        if n > MAX_CANDIDATES {
            return None;
        }
        Some(
            names
                .iter()
                .filter(|&&n| Vm::is_str(n) && keep(self.str_ref(n)))
                .map(|&n| String::from(self.str_ref(n)))
                .collect(),
        )
    }

    /// Records the "Did you mean" hint of an `AttributeError` on `obj.name`
    /// (pending until rendered).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn set_attr_hint(&mut self, obj: Value, name: Value) {
        self.set_hint_pending(PENDING_ATTR, name, obj, Value::NONE, Value::NONE);
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn attr_hint_text(&mut self, obj: Value, name: Value) -> Option<String> {
        let wrong = String::from(self.str_ref(name));
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
            let mut builtin = Vec::new();
            for k in self.mro_of(c) {
                if Vm::is_class(k) {
                    let d = self.vslot(k, crate::class::cls::DICT);
                    names.extend(self.dict_items_vec(d).into_iter().map(|(n, _)| n));
                } else if let Some(t) = self.type_of_typeobj(k) {
                    builtin.extend(crate::builtins::method_names(t));
                }
            }
            let owned = self.hint_names(&names, &wrong)?;
            // `dir(obj)`, sorted (the first of equally near names wins).
            let mut cands: Vec<&str> = owned.iter().map(String::as_str).collect();
            cands.extend(builtin);
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
        let n = self.str_ref_hot(name);
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
    /// `(globals, locals)`, when given, then the module note: "forget to
    /// import" for a module this build imports, the redirect sentence for
    /// one the sandbox leaves out (docs/user-guide.md, "Did you forget to
    /// import").
    #[cfg_attr(mm_shipped, optimize(size))]
    fn name_hint_text(&mut self, name: &str, frame: Option<(Value, Value)>) -> Option<String> {
        let mut hint = String::new();
        let mut s = None;
        if let Some((globals, locals)) = frame {
            if Vm::is_str(locals) && self.str_ref(locals) == SELF_HAS_NAME {
                let _ = write!(hint, ". Did you mean: 'self.{name}'?");
                s = Some(());
            } else {
                let mut owned: Vec<String> = if Vm::is_str(locals) {
                    self.str_ref(locals).split('\n').map(String::from).collect()
                } else {
                    Vec::new()
                };
                for (k, _) in self.dict_items_vec(globals) {
                    owned.push(self.str_ref(k).to_string());
                }
                for (k, _) in self.dict_items_vec(self.builtins) {
                    owned.push(self.str_ref(k).to_string());
                }
                for n in crate::prelude::AS_BUILTINS {
                    if !owned.iter().any(|o| o == n) {
                        owned.push(n.to_string());
                    }
                }
                let cands: Vec<&str> = owned
                    .iter()
                    .map(String::as_str)
                    .chain(["False", "None", "True"])
                    .collect();
                if let Some(c) = self.suggest_charged(&cands, name) {
                    let _ = write!(hint, ". Did you mean: '{c}'?");
                    s = Some(());
                }
            }
        }
        // CPython hints any of `sys.stdlib_module_names`. Here only a module
        // `import` finds in this build is named, so the hint never leads to
        // a failing import; a module the sandbox leaves out gets the
        // sentence its import would add, and any other name nothing.
        let importable =
            crate::BUNDLED.contains(&name) || self.stdlib.iter().any(|m| m.name == name);
        if importable {
            if s.is_some() {
                let _ = write!(hint, " Or did you forget to import '{name}'?");
            } else {
                let _ = write!(hint, ". Did you forget to import '{name}'?");
            }
        } else if let Some(g) = [
            HintGroup::Network,
            HintGroup::Processes,
            HintGroup::Filesystem,
        ]
        .into_iter()
        .find(|g| g.modules().contains(&name))
        {
            let text = self.hint_text(g, name);
            if !text.is_empty() {
                let _ = write!(hint, "{} {text}", if s.is_some() { "" } else { "." });
            }
        }
        (!hint.is_empty()).then_some(hint)
    }

    /// `(type name, str(exc))`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn exception_parts(&mut self, exc: Value) -> (String, String) {
        let c = self.vslot(exc, exc_slot::TYPE);
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

    #[cfg_attr(mm_shipped, optimize(size))]
    fn final_line(&mut self, exc: Value) -> String {
        let with_tb = !self.tb_entries(exc).is_empty();
        let mut line = if self.exc_type(exc).is_subclass(T::SyntaxError) {
            let c = self.vslot(exc, exc_slot::TYPE);
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
                line.push_str(self.str_ref(hint));
            } else if self.forgot_await(exc) {
                line.push_str(crate::hints::AWAIT_HINT);
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
                for l in self.str_ref(n).split('\n') {
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
    #[cfg_attr(mm_shipped, optimize(size))]
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
                self.stringify(file).map_or_default(|s| s.text)
            } else {
                String::from("<string>")
            };
            let l = self.stringify(line).map_or_default(|s| s.text);
            let _ = writeln!(out, "  File \"{f}\", line {l}");
        } else if file != Value::NONE {
            let f = self.stringify(file).map_or_default(|s| s.text);
            suffix = format!(" ({f})");
        }
        let mut cut = false;
        if Vm::is_str(text) {
            // Char counts, without a char copy of a text that can be as
            // long as the memory limit allows.
            let full = self.str_ref(text);
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
                    } else {
                        // Left out, raising nothing: the refusal dies.
                        self.heap.take_refused();
                    }
                }
            }
        }
        self.report_cut |= cut;
        let msg = self.syntax_attr(exc, "msg");
        let msg = if self.truthy(msg) {
            self.stringify(msg).map_or_default(|s| s.text)
        } else {
            String::from("<no detail available>")
        };
        let _ = write!(out, "{t}: {msg}{suffix}");
        out
    }

    /// `exc`'s traceback entries `(code, pc)`, innermost first.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn tb_entries(&self, exc: Value) -> Vec<(u32, u32)> {
        let tb = self.vslot(exc, exc_slot::TRACEBACK);
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
    #[cfg_attr(mm_shipped, optimize(size))]
    fn new_traceback(&mut self, exc: Value, k: usize) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.alloc_init(ty::TRACEBACK, 0, &[exc], &[k as u64]);
            self.hx(res)
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
    #[cfg_attr(mm_shipped, optimize(size))]
    fn traceback_parts(&self, tb: Value) -> Option<(Value, usize)> {
        let r = tb
            .as_obj()
            .filter(|_| tb.is_obj_kind(ty::TRACEBACK as u8))?;
        Some((
            self.slot(r, 0),
            usize::try_from(self.raw(r, 1)).unwrap_or(usize::MAX),
        ))
    }

    /// `exc.__traceback__ = v`: `None` empties the traceback; a traceback
    /// object makes it the entries from that one inward.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn set_traceback(&mut self, exc: Value, v: Value) -> Result<(), Raised> {
        let Some(r) = exc.as_obj() else { return Ok(()) };
        if v == Value::NONE {
            self.set_slot(r, exc_slot::TRACEBACK, Value::NONE);
            return Ok(());
        }
        let Some((owner, k)) = self.traceback_parts(v) else {
            return Err(self.err(T::TypeError, "__traceback__ must be a traceback or None"));
        };
        let tb = self.vslot(owner, exc_slot::TRACEBACK);
        let mut items = self.list_items(tb);
        items.truncate(k.saturating_add(1));
        self.burn(items.len() as u64);
        if owner == exc && tb.is_obj_kind(ty::LIST as u8) {
            self.list_set_items(tb, &items)
        } else {
            let l = self.new_list(&items)?;
            self.set_slot(r, exc_slot::TRACEBACK, l);
            Ok(())
        }
    }

    /// A traceback object's attributes.
    #[cfg_attr(mm_shipped, optimize(size))]
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
    #[cfg_attr(mm_shipped, optimize(size))]
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
            let count = usize::try_from(n.unsigned_abs()).unwrap_or(usize::MAX);
            if n >= 0 {
                frames.truncate(count);
            } else {
                frames.drain(..frames.len().saturating_sub(count));
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
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn render_exception(&mut self, exc: Value) -> String {
        let mut out = String::new();
        let mut need_close = false;
        self.render_into(exc, 0, &mut need_close, &mut out);
        out
    }

    /// The tuple of sub-exceptions of an `ExceptionGroup` (the prelude
    /// class keeps them in `_mm_excs`), `None` for other exceptions.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn group_items(&self, e: Value) -> Option<Value> {
        let k = self.find_interned("_mm_excs")?;
        let t = self.inst_get(e, k)?;
        t.is_obj_kind(ty::TUPLE as u8).then_some(t)
    }

    /// `exc`'s chain at exception-group depth `depth`. The recursion into
    /// sub-exceptions stops at CPython's `max_group_depth` (10), so it is
    /// bounded.
    #[cfg_attr(mm_shipped, optimize(size))]
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
            let cause = self.vslot(e, exc_slot::CAUSE);
            let context = self.vslot(e, exc_slot::CONTEXT);
            let suppress = e
                .as_obj()
                .is_some_and(|r| self.raw(r, exc_slot::FLAGS) & 1 != 0);
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
            // The first `MAX_WIDTH` are shown: read without a copy of the
            // rest.
            let len = self.tuple_len(items);
            let n = len.min(MAX_WIDTH + 1);
            *need_close = false;
            for j in 0..n {
                let sub = self.vslot(items, j as u32);
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
                    let more = len - MAX_WIDTH;
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

    #[cfg_attr(mm_shipped, optimize(size))]
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

    #[cfg_attr(mm_shipped, optimize(size))]
    fn frame_key(&self, ci: u32, pc: u32) -> (String, u32, String) {
        let Some(c) = self.codes.get(ci as usize) else {
            return (String::new(), 0, String::new());
        };
        let line = c.code.position(pc).map_or(0, |p| p.pos.line);
        let file = self.source_of(ci).map_or_default(|s| s.name.clone());
        (file, line, String::from(&*c.code.name))
    }

    /// One frame of a traceback: header, source lines and carets.
    /// The byte ranges of the lines of code `ci`'s source, split once per
    /// traceback and source (once per frame was O(frames × source)).
    #[cfg_attr(mm_shipped, optimize(size))]
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
    #[cfg_attr(mm_shipped, optimize(size))]
    fn source_line(&self, ci: u32, line: u32, cache: &mut LineCache) -> Option<&str> {
        let &(a, b) = self
            .line_ranges(ci, cache)
            .get((line as usize).checked_sub(1)?)?;
        self.source_of(ci)?.text.get(a..b)
    }

    #[allow(clippy::too_many_lines, reason = "a port of format_frame_summary")]
    #[cfg_attr(mm_shipped, optimize(size))]
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::panic, reason = "tests")]
    use super::*;
    use crate::vm::{Exit, StdModule, VmConfig};
    use alloc::boxed::Box;

    /// The last line of the report of `src`'s uncaught exception, run on a
    /// VM whose stdlib is `stdlib`.
    fn report(stdlib: &'static [StdModule], src: &str) -> String {
        let compile = |src: &str| {
            mm_compile::compile(
                &mm_parse::parse(src).expect("parses"),
                mm_compile::Options::default(),
            )
            .expect("compiles")
        };
        let mut vm = Vm::new(VmConfig {
            stdlib,
            ..VmConfig::default()
        })
        .expect("vm");
        let entry = vm.load(&compile(src), "t.py", src).expect("loads");
        let exit = vm.run(entry);
        assert!(matches!(exit, Exit::Error), "{src}: {exit:?}");
        let exc = vm.exception();
        let text = vm.render_exception(exc);
        String::from(text.lines().last().unwrap_or_default())
    }

    /// The hint names a module `import` finds in this session: one the
    /// session's stdlib adds is named, the same name without it is not.
    #[test]
    fn a_name_error_names_a_module_only_where_it_imports() {
        let program = mm_compile::compile(
            &mm_parse::parse("x = 1\n").expect("parses"),
            mm_compile::Options::default(),
        )
        .expect("compiles");
        let stdlib: &'static [StdModule] = Box::leak(Box::new([StdModule {
            name: "cmd",
            code: Box::leak(mm_compile::pack::pack(&program.encode()).into_boxed_slice()),
            packing: None,
        }]));
        assert_eq!(report(&[], "cmd"), "NameError: name 'cmd' is not defined");
        assert_eq!(
            report(stdlib, "cmd"),
            "NameError: name 'cmd' is not defined. Did you forget to import 'cmd'?"
        );
        // `math` is the engine's own, in every build.
        assert_eq!(
            report(&[], "math"),
            "NameError: name 'math' is not defined. Did you forget to import 'math'?"
        );
        assert_eq!(report(&[], "_mm"), "NameError: name '_mm' is not defined");
    }
}
