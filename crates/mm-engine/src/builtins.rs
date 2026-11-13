//! Builtin functions and methods: one static table, one calling convention
//! (`fn(&mut Vm, CallArgs) -> Result<Value, Raised>`), one keyword binder.

use crate::class::Dn;
use crate::cont::{Cont, check};
use crate::generator::ag;
use crate::nstep;
use crate::scratch::{NativeText, NativeValues};
use crate::text::FormatSpec;
use crate::types::{T, ty};
use crate::vm::{CallArgs, Raised, Request, Stream, Vm};
use alloc::{format, string::String, vec::Vec};
use core::cmp::Ordering;
use mm_core::Value;

/// The builtin calling convention.
pub type NativeFn = fn(&mut Vm, CallArgs) -> Result<Value, Raised>;

/// `max` for functions taking `*args`.
pub const VAR: u8 = u8::MAX;

/// One builtin as the table below writes it. Only compile-time code reads
/// these: [`NATIVES`] is built from them with the strings packed into one
/// blob, so the shipped table holds no pointers but the implementation.
struct Def {
    name: &'static str,
    owner: Option<T>,
    min: u8,
    max: u8,
    params: &'static [&'static str],
    kwonly: &'static [&'static str],
    module: Option<&'static str>,
    f: NativeFn,
}

/// One builtin function or method.
#[derive(Clone, Copy)]
pub struct Native {
    /// The type it is a method of, or `None` for a function.
    pub owner: Option<T>,
    /// Fewest positional arguments (the receiver included for methods).
    pub min: u8,
    /// Most positional arguments, or `VAR`.
    pub max: u8,
    /// 0, or 1 + an index into [`MODULES`].
    module: u8,
    name: Sref,
    /// The positional parameter names, then the keyword-only ones, in
    /// [`LISTS`].
    args: u16,
    nparams: u8,
    nkwonly: u8,
    /// The implementation.
    pub f: NativeFn,
}

/// Whether builtin method `n` takes a str subclass instance itself, not
/// the str it extends: `partition` and `rpartition` give the receiver
/// back when the separator is not found, as CPython's do.
pub(crate) fn keeps_instance(n: &Native) -> bool {
    match n.owner {
        Some(T::str) => matches!(n.name(), "partition" | "rpartition"),
        // Its view records that it was made through a subclass.
        Some(T::dict) => n.name() == "keys",
        _ => false,
    }
}

/// A string in [`BLOB`].
#[derive(Clone, Copy)]
struct Sref {
    off: u16,
    len: u8,
}

impl Sref {
    #[inline(never)]
    fn get(self) -> &'static str {
        let a = self.off as usize;
        BLOB.get(a..a + self.len as usize).unwrap_or("")
    }
}

/// A run of parameter names: `len` strings from `LISTS[start]`.
#[derive(Clone, Copy)]
pub struct Names {
    start: u16,
    len: u8,
}

impl Names {
    /// The names in order.
    pub fn iter(self) -> impl Iterator<Item = &'static str> + Clone {
        let a = self.start as usize;
        LISTS
            .get(a..a + self.len as usize)
            .unwrap_or(&[])
            .iter()
            .map(|s| s.get())
    }
    /// Name `i`.
    #[must_use]
    pub fn get(self, i: usize) -> Option<&'static str> {
        (i < self.len as usize)
            .then(|| LISTS.get(self.start as usize + i))
            .flatten()
            .map(|s| s.get())
    }
    /// Number of names.
    #[must_use]
    pub fn len(self) -> usize {
        self.len as usize
    }
    /// No names?
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
    /// Is `name` one of them?
    #[must_use]
    pub fn contains(self, name: &str) -> bool {
        self.iter().any(|p| p == name)
    }
}

impl Native {
    /// `__name__`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name.get()
    }
    /// The bundled module it belongs to (not a builtin name), if any.
    #[must_use]
    pub fn module(&self) -> Option<&'static str> {
        let i = usize::from(self.module).checked_sub(1)?;
        MODULES.get(i).map(|s| s.get())
    }
    /// Names of the positional parameters that may be passed by keyword
    /// (`""` for positional-only ones).
    #[must_use]
    pub fn params(&self) -> Names {
        Names {
            start: self.args,
            len: self.nparams,
        }
    }
    /// Keyword-only parameters; `"**"` accepts any keyword into a dict.
    #[must_use]
    pub fn kwonly(&self) -> Names {
        Names {
            start: self.args.wrapping_add(u16::from(self.nparams)),
            len: self.nkwonly,
        }
    }
}

macro_rules! n {
    ($name:literal, $owner:expr, $min:expr, $max:expr, $params:expr, $kw:expr, $f:expr) => {
        Def {
            name: $name,
            owner: $owner,
            min: $min,
            max: $max,
            params: $params,
            kwonly: $kw,
            module: None,
            f: $f,
        }
    };
}

/// A function of bundled module `$module`.
macro_rules! m {
    ($module:literal, $name:literal, $min:expr, $max:expr, $params:expr, $kw:expr, $f:expr) => {
        Def {
            name: $name,
            owner: None,
            min: $min,
            max: $max,
            params: $params,
            kwonly: $kw,
            module: Some($module),
            f: $f,
        }
    };
}

const NO: &[&str] = &[];
const L: Option<T> = Some(T::list);
const D: Option<T> = Some(T::dict);
const S: Option<T> = Some(T::str);
const TU: Option<T> = Some(T::tuple);
const RA: Option<T> = Some(T::range);
const SE: Option<T> = Some(T::set);
const FS: Option<T> = Some(T::frozenset);
const IO: Option<T> = Some(T::TextIOWrapper);
const OB: Option<T> = Some(T::object);
const BE: Option<T> = Some(T::BaseException);
const GE: Option<T> = Some(T::generator);
const AG: Option<T> = Some(T::async_generator);
const CO: Option<T> = Some(T::coroutine);
const CI: Option<T> = Some(T::callable_iterator);
#[cfg(feature = "re")]
const MA: Option<T> = Some(T::Match);
const PR: Option<T> = Some(T::property);
const BY: Option<T> = Some(T::bytes);
const BA: Option<T> = Some(T::bytearray);
const IN: Option<T> = Some(T::int);
const FL: Option<T> = Some(T::float);
const TY: Option<T> = Some(T::type_);
const CX: Option<T> = Some(T::complex);
#[cfg(feature = "asyncio")]
const TK: Option<T> = Some(T::Task);
const FU: Option<T> = Some(T::Future);
#[cfg(feature = "asyncio")]
const GA: Option<T> = Some(T::GatheringFuture);

/// Helpers only the prelude sees (module `_prelude`).
macro_rules! p {
    ($name:literal, $min:expr, $max:expr, $f:expr) => {
        m!("_prelude", $name, $min, $max, NO, NO, $f)
    };
}

/// Every builtin as written; [`NATIVES`] orders and packs them.
const DEFS: &[Def] = &[
    n!(
        "print",
        None,
        0,
        VAR,
        NO,
        &["sep", "end", "file", "flush"],
        bi_print
    ),
    n!("len", None, 1, 1, NO, NO, bi_len),
    n!("repr", None, 1, 1, NO, NO, bi_repr),
    n!("isinstance", None, 2, 2, NO, NO, bi_isinstance),
    n!("abs", None, 1, 1, NO, NO, bi_abs),
    n!("min", None, 1, VAR, NO, &["key", "default"], bi_min),
    n!("max", None, 1, VAR, NO, &["key", "default"], bi_max),
    n!("sum", None, 1, 2, &["", "start"], NO, bi_sum),
    n!("sorted", None, 1, 1, NO, &["key", "reverse"], bi_sorted),
    n!("any", None, 1, 1, NO, NO, bi_any),
    n!("all", None, 1, 1, NO, NO, bi_all),
    n!("hash", None, 1, 1, NO, NO, bi_hash),
    n!("id", None, 1, 1, NO, NO, bi_id),
    n!("iter", None, 1, 2, NO, NO, bi_iter),
    n!("next", None, 1, 2, NO, NO, bi_next),
    n!("aiter", None, 1, 1, NO, NO, bi_aiter),
    n!("anext", None, 1, 2, NO, NO, bi_anext),
    n!("reversed", None, 1, 1, NO, NO, bi_reversed),
    n!("round", None, 1, 2, &["number", "ndigits"], NO, bi_round),
    n!("divmod", None, 2, 2, NO, NO, bi_divmod),
    n!("pow", None, 2, 3, &["base", "exp", "mod"], NO, bi_pow),
    n!("chr", None, 1, 1, NO, NO, bi_chr),
    n!("ord", None, 1, 1, NO, NO, bi_ord),
    n!("bin", None, 1, 1, NO, NO, bi_bin),
    n!("hex", None, 1, 1, NO, NO, bi_hex),
    n!("oct", None, 1, 1, NO, NO, bi_oct),
    n!("callable", None, 1, 1, NO, NO, bi_callable),
    n!("format", None, 1, 2, NO, NO, bi_format),
    n!("getattr", None, 2, 3, NO, NO, bi_getattr),
    n!("hasattr", None, 2, 2, NO, NO, bi_hasattr),
    n!("vars", None, 1, 1, NO, NO, bi_vars),
    // `<scope>`: what a compiled zero-argument `dir()` passes (compiler).
    n!("dir", None, 0, 1, NO, &["<scope>"], bi_dir),
    n!("ascii", None, 1, 1, NO, NO, bi_ascii),
    n!("issubclass", None, 2, 2, NO, NO, bi_issubclass),
    n!("setattr", None, 3, 3, NO, NO, bi_setattr),
    n!("delattr", None, 2, 2, NO, NO, bi_delattr),
    n!(
        "property",
        None,
        0,
        4,
        &["fget", "fset", "fdel", "doc"],
        NO,
        bi_property
    ),
    n!("staticmethod", None, 1, 1, NO, NO, bi_staticmethod),
    n!("classmethod", None, 1, 1, NO, NO, bi_classmethod),
    n!("super", None, 0, 2, NO, NO, bi_super),
    // Type constructors (installed as the type objects, not by name).
    n!("int", None, 0, 2, &["", "base"], NO, bi_int),
    n!("float", None, 0, 1, NO, NO, bi_float),
    n!(
        "complex",
        None,
        0,
        2,
        &["real", "imag"],
        NO,
        crate::complex::bi_complex
    ),
    n!(
        "str",
        None,
        0,
        3,
        &["object", "encoding", "errors"],
        NO,
        bi_str
    ),
    n!(
        "bytes",
        None,
        0,
        3,
        &["source", "encoding", "errors"],
        NO,
        crate::bytes::bi_bytes
    ),
    n!(
        "bytearray",
        None,
        0,
        3,
        &["source", "encoding", "errors"],
        NO,
        crate::bytes::bi_bytearray
    ),
    n!("bool", None, 0, 1, NO, NO, bi_bool),
    n!("list", None, 0, 1, NO, NO, bi_list),
    n!("tuple", None, 0, 1, NO, NO, bi_tuple),
    n!("dict", None, 0, 1, NO, &["**"], bi_dict),
    n!("set", None, 0, 1, NO, NO, bi_set),
    n!("frozenset", None, 0, 1, NO, NO, bi_frozenset),
    n!("range", None, 1, 3, NO, NO, bi_range),
    n!("type", None, 0, VAR, NO, &["**"], bi_type),
    n!(
        "enumerate",
        None,
        1,
        2,
        &["iterable", "start"],
        NO,
        bi_enumerate
    ),
    n!("zip", None, 0, VAR, NO, &["strict"], bi_zip),
    n!("slice", None, 1, 3, NO, NO, bi_slice),
    n!("object", None, 0, 0, NO, NO, bi_object),
    // object
    n!("__init__", OB, 1, VAR, NO, &["**"], object_init),
    // Slot wrappers of builtin types (`list.__getitem__`, `super().__setitem__`
    // in a subclass): the VM operation on the (unwrapped) receiver.
    n!("__getitem__", L, 2, 2, NO, NO, slot_getitem),
    n!("__setitem__", L, 3, 3, NO, NO, slot_setitem),
    n!("__delitem__", L, 2, 2, NO, NO, slot_delitem),
    n!("__len__", L, 1, 1, NO, NO, slot_len),
    n!("__contains__", L, 2, 2, NO, NO, slot_contains),
    n!("__iter__", L, 1, 1, NO, NO, slot_iter),
    n!("__reversed__", L, 1, 1, NO, NO, slot_reversed),
    n!("__repr__", L, 1, 1, NO, NO, slot_repr),
    n!("__eq__", L, 2, 2, NO, NO, slot_eq),
    n!("__ne__", L, 2, 2, NO, NO, slot_ne),
    n!("__lt__", L, 2, 2, NO, NO, slot_lt),
    n!("__le__", L, 2, 2, NO, NO, slot_le),
    n!("__gt__", L, 2, 2, NO, NO, slot_gt),
    n!("__ge__", L, 2, 2, NO, NO, slot_ge),
    n!("__getitem__", D, 2, 2, NO, NO, slot_getitem),
    n!("__setitem__", D, 3, 3, NO, NO, slot_setitem),
    n!("__delitem__", D, 2, 2, NO, NO, slot_delitem),
    n!("__len__", D, 1, 1, NO, NO, slot_len),
    n!("__contains__", D, 2, 2, NO, NO, slot_contains),
    n!("__iter__", D, 1, 1, NO, NO, slot_iter),
    n!("__reversed__", D, 1, 1, NO, NO, slot_reversed),
    n!("__repr__", D, 1, 1, NO, NO, slot_repr),
    n!("__eq__", D, 2, 2, NO, NO, slot_eq),
    n!("__ne__", D, 2, 2, NO, NO, slot_ne),
    n!("__getitem__", TU, 2, 2, NO, NO, slot_getitem),
    n!("__len__", TU, 1, 1, NO, NO, slot_len),
    n!("__contains__", TU, 2, 2, NO, NO, slot_contains),
    n!("__iter__", TU, 1, 1, NO, NO, slot_iter),
    n!("__repr__", TU, 1, 1, NO, NO, slot_repr),
    n!("__eq__", TU, 2, 2, NO, NO, slot_eq),
    n!("__ne__", TU, 2, 2, NO, NO, slot_ne),
    n!("__lt__", TU, 2, 2, NO, NO, slot_lt),
    n!("__le__", TU, 2, 2, NO, NO, slot_le),
    n!("__gt__", TU, 2, 2, NO, NO, slot_gt),
    n!("__ge__", TU, 2, 2, NO, NO, slot_ge),
    n!("__hash__", TU, 1, 1, NO, NO, slot_hash),
    n!("__getitem__", S, 2, 2, NO, NO, slot_getitem),
    n!("__len__", S, 1, 1, NO, NO, slot_len),
    n!("__contains__", S, 2, 2, NO, NO, slot_contains),
    n!("__iter__", S, 1, 1, NO, NO, slot_iter),
    n!("__repr__", S, 1, 1, NO, NO, slot_repr),
    n!("__str__", S, 1, 1, NO, NO, str_str),
    n!("__eq__", S, 2, 2, NO, NO, slot_eq),
    n!("__ne__", S, 2, 2, NO, NO, slot_ne),
    n!("__lt__", S, 2, 2, NO, NO, slot_lt),
    n!("__le__", S, 2, 2, NO, NO, slot_le),
    n!("__gt__", S, 2, 2, NO, NO, slot_gt),
    n!("__ge__", S, 2, 2, NO, NO, slot_ge),
    n!("__hash__", S, 1, 1, NO, NO, slot_hash),
    n!("__len__", SE, 1, 1, NO, NO, slot_len),
    n!("__contains__", SE, 2, 2, NO, NO, slot_contains),
    n!("__iter__", SE, 1, 1, NO, NO, slot_iter),
    n!("__repr__", SE, 1, 1, NO, NO, slot_repr),
    n!("__eq__", SE, 2, 2, NO, NO, slot_eq),
    n!("__ne__", SE, 2, 2, NO, NO, slot_ne),
    n!("__lt__", SE, 2, 2, NO, NO, slot_lt),
    n!("__le__", SE, 2, 2, NO, NO, slot_le),
    n!("__gt__", SE, 2, 2, NO, NO, slot_gt),
    n!("__ge__", SE, 2, 2, NO, NO, slot_ge),
    n!("__len__", FS, 1, 1, NO, NO, slot_len),
    n!("__contains__", FS, 2, 2, NO, NO, slot_contains),
    n!("__iter__", FS, 1, 1, NO, NO, slot_iter),
    n!("__repr__", FS, 1, 1, NO, NO, slot_repr),
    n!("__eq__", FS, 2, 2, NO, NO, slot_eq),
    n!("__ne__", FS, 2, 2, NO, NO, slot_ne),
    n!("__lt__", FS, 2, 2, NO, NO, slot_lt),
    n!("__le__", FS, 2, 2, NO, NO, slot_le),
    n!("__gt__", FS, 2, 2, NO, NO, slot_gt),
    n!("__ge__", FS, 2, 2, NO, NO, slot_ge),
    n!("__hash__", FS, 1, 1, NO, NO, slot_hash),
    n!("__getitem__", BY, 2, 2, NO, NO, slot_getitem),
    n!("__len__", BY, 1, 1, NO, NO, slot_len),
    n!("__contains__", BY, 2, 2, NO, NO, slot_contains),
    n!("__iter__", BY, 1, 1, NO, NO, slot_iter),
    n!("__repr__", BY, 1, 1, NO, NO, slot_repr),
    n!("__eq__", BY, 2, 2, NO, NO, slot_eq),
    n!("__ne__", BY, 2, 2, NO, NO, slot_ne),
    n!("__lt__", BY, 2, 2, NO, NO, slot_lt),
    n!("__le__", BY, 2, 2, NO, NO, slot_le),
    n!("__gt__", BY, 2, 2, NO, NO, slot_gt),
    n!("__ge__", BY, 2, 2, NO, NO, slot_ge),
    n!("__hash__", BY, 1, 1, NO, NO, slot_hash),
    n!("__repr__", IN, 1, 1, NO, NO, slot_repr),
    n!("__eq__", IN, 2, 2, NO, NO, slot_eq),
    n!("__ne__", IN, 2, 2, NO, NO, slot_ne),
    n!("__lt__", IN, 2, 2, NO, NO, slot_lt),
    n!("__le__", IN, 2, 2, NO, NO, slot_le),
    n!("__gt__", IN, 2, 2, NO, NO, slot_gt),
    n!("__ge__", IN, 2, 2, NO, NO, slot_ge),
    n!("__hash__", IN, 1, 1, NO, NO, slot_hash),
    n!("__repr__", FL, 1, 1, NO, NO, slot_repr),
    n!("__eq__", FL, 2, 2, NO, NO, slot_eq),
    n!("__ne__", FL, 2, 2, NO, NO, slot_ne),
    n!("__lt__", FL, 2, 2, NO, NO, slot_lt),
    n!("__le__", FL, 2, 2, NO, NO, slot_le),
    n!("__gt__", FL, 2, 2, NO, NO, slot_gt),
    n!("__ge__", FL, 2, 2, NO, NO, slot_ge),
    n!("__hash__", FL, 1, 1, NO, NO, slot_hash),
    n!("__init__", L, 1, 2, NO, NO, list_init),
    n!("__init__", D, 1, 2, NO, &["**"], dict_update),
    n!("__init__", SE, 1, 2, NO, NO, set_init),
    n!("__new__", TU, 1, VAR, NO, &["**"], tuple_new),
    n!("__new__", L, 1, VAR, NO, &["**"], list_new),
    n!("__new__", D, 1, VAR, NO, &["**"], dict_new),
    n!("__new__", SE, 1, VAR, NO, &["**"], set_new),
    n!("__new__", IN, 1, VAR, NO, &["**"], int_new),
    n!("__new__", S, 1, VAR, NO, &["**"], str_new),
    n!("__new__", FL, 1, VAR, NO, &["**"], float_new),
    n!("__new__", BY, 1, VAR, NO, &["**"], bytes_new),
    n!("__new__", OB, 1, VAR, NO, &["**"], object_new),
    n!("__repr__", OB, 1, 1, NO, NO, object_repr),
    n!("__str__", OB, 1, 1, NO, NO, object_str),
    n!("__format__", OB, 2, 2, NO, NO, object_format),
    n!("__eq__", OB, 2, 2, NO, NO, object_eq),
    n!("__ne__", OB, 2, 2, NO, NO, object_ne),
    n!("__hash__", OB, 1, 1, NO, NO, object_hash),
    n!("__setattr__", OB, 3, 3, NO, NO, object_setattr),
    n!("__getattribute__", OB, 2, 2, NO, NO, object_getattribute),
    n!("__delattr__", OB, 2, 2, NO, NO, object_delattr),
    n!(
        "__init_subclass__",
        OB,
        1,
        1,
        NO,
        &["**"],
        object_init_subclass
    ),
    n!("mro", TY, 1, 1, NO, NO, type_mro),
    // `_object___dir__`, `_type___dir__` and `_module___dir__` in the prelude.
    n!("__dir__", OB, 1, 1, NO, NO, to_module_twin),
    n!("__dir__", TY, 1, 1, NO, NO, to_module_twin),
    n!("__dir__", Some(T::module), 1, 1, NO, NO, to_module_twin),
    n!("__new__", TY, 4, 4, NO, &["**"], type_dunder_new),
    n!("__init__", TY, 1, VAR, NO, &["**"], type_dunder_init),
    n!("__call__", TY, 1, VAR, NO, &["**"], type_dunder_call),
    n!("__setattr__", TY, 3, 3, NO, NO, object_setattr),
    n!("__delattr__", TY, 2, 2, NO, NO, object_delattr),
    // BaseException
    n!("__init__", BE, 1, VAR, NO, &["**"], exc_init),
    n!("__str__", BE, 1, 1, NO, NO, exc_str_m),
    n!("__repr__", BE, 1, 1, NO, NO, object_repr),
    n!("with_traceback", BE, 2, 2, NO, NO, exc_with_traceback),
    n!("add_note", BE, 2, 2, NO, NO, exc_add_note),
    // generator
    n!("send", GE, 2, 2, NO, NO, gen_send),
    n!("throw", GE, 2, 4, NO, NO, gen_throw),
    n!("close", GE, 1, 1, NO, NO, gen_close),
    n!("__next__", GE, 1, 1, NO, NO, gen_next),
    n!("__iter__", GE, 1, 1, NO, NO, self_iter),
    n!("__aiter__", AG, 1, 1, NO, NO, self_iter),
    n!("__anext__", AG, 1, 1, NO, NO, agen_anext),
    n!("asend", AG, 2, 2, NO, NO, agen_asend),
    n!("athrow", AG, 2, 4, NO, NO, agen_athrow),
    n!("aclose", AG, 1, 1, NO, NO, agen_aclose),
    // coroutine
    n!("send", CO, 2, 2, NO, NO, coro_send),
    n!("throw", CO, 2, 4, NO, NO, coro_throw),
    n!("close", CO, 1, 1, NO, NO, coro_close),
    n!("__await__", CO, 1, 1, NO, NO, coro_await),
    n!("__next__", CI, 1, 1, NO, NO, gen_next),
    n!("__iter__", CI, 1, 1, NO, NO, self_iter),
    n!(
        "__length_hint__",
        Some(T::range_iterator),
        1,
        1,
        NO,
        NO,
        range_hint
    ),
    // property
    n!("__get__", PR, 2, 3, NO, NO, descr_get),
    n!("__get__", Some(T::function), 2, 3, NO, NO, descr_get),
    n!("__get__", Some(T::classmethod), 2, 3, NO, NO, descr_get),
    n!("__get__", Some(T::staticmethod), 2, 3, NO, NO, descr_get),
    n!("getter", PR, 2, 2, NO, NO, prop_getter),
    n!("setter", PR, 2, 2, NO, NO, prop_setter),
    n!("deleter", PR, 2, 2, NO, NO, prop_deleter),
    // complex
    n!(
        "conjugate",
        CX,
        1,
        1,
        NO,
        NO,
        crate::complex::complex_conjugate
    ),
    n!(
        "__complex__",
        CX,
        1,
        1,
        NO,
        NO,
        crate::complex::complex_complex
    ),
    n!(
        "from_number",
        CX,
        1,
        VAR,
        NO,
        NO,
        crate::complex::complex_from_number
    ),
    // list
    n!("append", L, 2, 2, NO, NO, list_append),
    n!("extend", L, 2, 2, NO, NO, list_extend),
    n!("pop", L, 1, 2, NO, NO, list_pop),
    n!("insert", L, 3, 3, NO, NO, list_insert),
    n!("remove", L, 2, 2, NO, NO, list_remove),
    n!("index", L, 2, 4, NO, NO, seq_index),
    n!("count", L, 2, 2, NO, NO, seq_count),
    n!("sort", L, 1, 1, NO, &["key", "reverse"], list_sort),
    n!("reverse", L, 1, 1, NO, NO, list_reverse),
    n!("copy", L, 1, 1, NO, NO, list_copy),
    n!("clear", L, 1, 1, NO, NO, list_clear),
    // tuple
    n!("index", TU, 2, 4, NO, NO, seq_index),
    n!("count", TU, 2, 2, NO, NO, seq_count),
    // range
    n!("index", RA, 2, 2, NO, NO, range_index),
    n!("count", RA, 2, 2, NO, NO, range_count),
    // dict
    n!("get", D, 2, 3, NO, NO, dict_get),
    n!("pop", D, 2, 3, NO, NO, dict_pop),
    n!("keys", D, 1, 1, NO, NO, dict_keys),
    n!("values", D, 1, 1, NO, NO, dict_values),
    n!("items", D, 1, 1, NO, NO, dict_items),
    n!("setdefault", D, 2, 3, NO, NO, dict_setdefault),
    n!("update", D, 1, 2, NO, &["**"], dict_update),
    n!("copy", D, 1, 1, NO, NO, dict_copy),
    n!("clear", D, 1, 1, NO, NO, dict_clear),
    n!("popitem", D, 1, 1, NO, NO, dict_popitem),
    n!("fromkeys", D, 2, 3, NO, NO, dict_fromkeys),
    // set
    n!("add", SE, 2, 2, NO, NO, set_add),
    n!("remove", SE, 2, 2, NO, NO, set_remove),
    n!("discard", SE, 2, 2, NO, NO, set_discard),
    n!("pop", SE, 1, 1, NO, NO, set_pop),
    n!("union", SE, 1, VAR, NO, NO, set_union),
    n!("intersection", SE, 1, VAR, NO, NO, set_intersection),
    n!("difference", SE, 1, VAR, NO, NO, set_difference),
    n!("update", SE, 1, VAR, NO, NO, set_update),
    n!("union", FS, 1, VAR, NO, NO, set_union),
    n!("intersection", FS, 1, VAR, NO, NO, set_intersection),
    n!("difference", FS, 1, VAR, NO, NO, set_difference),
    n!("issubset", FS, 2, 2, NO, NO, set_issubset),
    n!("issuperset", FS, 2, 2, NO, NO, set_issuperset),
    n!("isdisjoint", FS, 2, 2, NO, NO, set_isdisjoint),
    n!("copy", FS, 1, 1, NO, NO, set_copy),
    n!(
        "symmetric_difference",
        FS,
        2,
        2,
        NO,
        NO,
        set_symmetric_difference
    ),
    n!("issubset", SE, 2, 2, NO, NO, set_issubset),
    n!("issuperset", SE, 2, 2, NO, NO, set_issuperset),
    n!(
        "symmetric_difference",
        SE,
        2,
        2,
        NO,
        NO,
        set_symmetric_difference
    ),
    n!(
        "symmetric_difference_update",
        SE,
        2,
        2,
        NO,
        NO,
        set_symmetric_difference_update
    ),
    n!(
        "intersection_update",
        SE,
        1,
        VAR,
        NO,
        NO,
        set_intersection_update
    ),
    n!(
        "difference_update",
        SE,
        1,
        VAR,
        NO,
        NO,
        set_difference_update
    ),
    n!("isdisjoint", SE, 2, 2, NO, NO, set_isdisjoint),
    n!(
        "isdisjoint",
        Some(T::dict_keys),
        2,
        2,
        NO,
        NO,
        view_isdisjoint
    ),
    n!(
        "isdisjoint",
        Some(T::dict_items),
        2,
        2,
        NO,
        NO,
        view_isdisjoint
    ),
    n!("copy", SE, 1, 1, NO, NO, set_copy),
    n!("clear", SE, 1, 1, NO, NO, set_clear),
    // str
    n!("join", S, 2, 2, NO, NO, crate::strmeth::join),
    n!(
        "split",
        S,
        1,
        3,
        &["", "sep", "maxsplit"],
        NO,
        crate::strmeth::split
    ),
    n!(
        "rsplit",
        S,
        1,
        3,
        &["", "sep", "maxsplit"],
        NO,
        crate::strmeth::rsplit
    ),
    n!("strip", S, 1, 2, NO, NO, crate::strmeth::strip),
    n!("lstrip", S, 1, 2, NO, NO, crate::strmeth::lstrip),
    n!("rstrip", S, 1, 2, NO, NO, crate::strmeth::rstrip),
    n!("startswith", S, 2, 4, NO, NO, crate::strmeth::startswith),
    n!("endswith", S, 2, 4, NO, NO, crate::strmeth::endswith),
    n!(
        "replace",
        S,
        3,
        4,
        &["", "", "", "count"],
        NO,
        crate::strmeth::replace
    ),
    n!("upper", S, 1, 1, NO, NO, crate::strmeth::upper),
    n!("lower", S, 1, 1, NO, NO, crate::strmeth::lower),
    n!("find", S, 2, 4, NO, NO, crate::strmeth::find),
    n!("rfind", S, 2, 4, NO, NO, crate::strmeth::rfind),
    n!("index", S, 2, 4, NO, NO, crate::strmeth::index),
    n!("rindex", S, 2, 4, NO, NO, crate::strmeth::rindex),
    n!("count", S, 2, 4, NO, NO, crate::strmeth::count),
    n!("isdigit", S, 1, 1, NO, NO, crate::strmeth::isdigit),
    n!("isdecimal", S, 1, 1, NO, NO, crate::strmeth::isdecimal),
    n!("isnumeric", S, 1, 1, NO, NO, crate::strmeth::isnumeric),
    n!("isalpha", S, 1, 1, NO, NO, crate::strmeth::isalpha),
    n!("isalnum", S, 1, 1, NO, NO, crate::strmeth::isalnum),
    n!(
        "isidentifier",
        S,
        1,
        1,
        NO,
        NO,
        crate::strmeth::isidentifier
    ),
    n!("isspace", S, 1, 1, NO, NO, crate::strmeth::isspace),
    n!("isprintable", S, 1, 1, NO, NO, crate::strmeth::isprintable),
    n!("isascii", S, 1, 1, NO, NO, crate::strmeth::isascii),
    n!("istitle", S, 1, 1, NO, NO, crate::strmeth::istitle),
    n!("isupper", S, 1, 1, NO, NO, crate::strmeth::isupper),
    n!("islower", S, 1, 1, NO, NO, crate::strmeth::islower),
    n!("title", S, 1, 1, NO, NO, crate::strmeth::title),
    n!("capitalize", S, 1, 1, NO, NO, crate::strmeth::capitalize),
    n!("casefold", S, 1, 1, NO, NO, crate::strmeth::casefold),
    n!("zfill", S, 2, 2, NO, NO, crate::strmeth::zfill),
    n!(
        "expandtabs",
        S,
        1,
        2,
        &["", "tabsize"],
        NO,
        crate::strmeth::expandtabs
    ),
    n!("center", S, 2, 3, NO, NO, crate::strmeth::center),
    n!("ljust", S, 2, 3, NO, NO, crate::strmeth::ljust),
    n!("rjust", S, 2, 3, NO, NO, crate::strmeth::rjust),
    n!(
        "splitlines",
        S,
        1,
        2,
        &["", "keepends"],
        NO,
        crate::strmeth::splitlines
    ),
    n!("partition", S, 2, 2, NO, NO, crate::strmeth::partition),
    n!("rpartition", S, 2, 2, NO, NO, crate::strmeth::rpartition),
    n!("format", S, 1, VAR, NO, &["**"], crate::strmeth::format),
    n!(
        "removeprefix",
        S,
        2,
        2,
        NO,
        NO,
        crate::strmeth::removeprefix
    ),
    n!(
        "removesuffix",
        S,
        2,
        2,
        NO,
        NO,
        crate::strmeth::removesuffix
    ),
    n!("swapcase", S, 1, 1, NO, NO, crate::strmeth::swapcase),
    n!("translate", S, 2, 2, NO, NO, crate::strmeth::translate),
    n!("maketrans", S, 2, 4, NO, NO, to_module_twin),
    n!("format_map", S, 2, 2, NO, NO, to_module_twin),
    n!(
        "encode",
        S,
        1,
        3,
        &["", "encoding", "errors"],
        NO,
        crate::bytes::str_encode
    ),
    // bytes
    n!(
        "decode",
        BY,
        1,
        3,
        &["", "encoding", "errors"],
        NO,
        crate::bytes::decode
    ),
    n!(
        "hex",
        BY,
        1,
        3,
        &["", "sep", "bytes_per_sep"],
        NO,
        crate::bytes::hex
    ),
    n!("fromhex", BY, 2, 2, NO, NO, crate::bytes::fromhex),
    n!(
        "translate",
        BY,
        2,
        3,
        &["", "", "delete"],
        NO,
        crate::bytes::translate
    ),
    n!("maketrans", BY, 3, 3, NO, NO, crate::bytes::maketrans),
    n!(
        "expandtabs",
        BY,
        1,
        2,
        &["", "tabsize"],
        NO,
        crate::bytes::expandtabs
    ),
    // bytearray: the mutators; the rest is bytes' run (`method_index`).
    n!("__setitem__", BA, 3, 3, NO, NO, slot_setitem),
    n!("__delitem__", BA, 2, 2, NO, NO, slot_delitem),
    n!("append", BA, 2, 2, NO, NO, crate::bytearray::append),
    n!("extend", BA, 2, 2, NO, NO, crate::bytearray::extend),
    n!("insert", BA, 3, 3, NO, NO, crate::bytearray::insert),
    n!("pop", BA, 1, 2, NO, NO, crate::bytearray::pop),
    n!("remove", BA, 2, 2, NO, NO, crate::bytearray::remove),
    n!("clear", BA, 1, 1, NO, NO, crate::bytearray::clear),
    n!("reverse", BA, 1, 1, NO, NO, crate::bytearray::reverse),
    n!("copy", BA, 1, 1, NO, NO, crate::bytearray::copy),
    n!("find", BY, 2, 4, NO, NO, crate::bytes::find),
    n!("rfind", BY, 2, 4, NO, NO, crate::bytes::rfind),
    n!("index", BY, 2, 4, NO, NO, crate::bytes::index),
    n!("rindex", BY, 2, 4, NO, NO, crate::bytes::rindex),
    n!("count", BY, 2, 4, NO, NO, crate::bytes::count),
    n!("startswith", BY, 2, 4, NO, NO, crate::bytes::startswith),
    n!("endswith", BY, 2, 4, NO, NO, crate::bytes::endswith),
    n!(
        "split",
        BY,
        1,
        3,
        &["", "sep", "maxsplit"],
        NO,
        crate::bytes::split
    ),
    n!(
        "rsplit",
        BY,
        1,
        3,
        &["", "sep", "maxsplit"],
        NO,
        crate::bytes::rsplit
    ),
    n!("join", BY, 2, 2, NO, NO, crate::bytes::join),
    n!("strip", BY, 1, 2, NO, NO, crate::bytes::strip),
    n!("lstrip", BY, 1, 2, NO, NO, crate::bytes::lstrip),
    n!("rstrip", BY, 1, 2, NO, NO, crate::bytes::rstrip),
    n!(
        "replace",
        BY,
        3,
        4,
        &["", "", "", "count"],
        NO,
        crate::bytes::replace
    ),
    n!("upper", BY, 1, 1, NO, NO, crate::bytes::upper),
    n!("lower", BY, 1, 1, NO, NO, crate::bytes::lower),
    n!("swapcase", BY, 1, 1, NO, NO, crate::bytes::swapcase),
    n!("capitalize", BY, 1, 1, NO, NO, crate::bytes::capitalize),
    n!("title", BY, 1, 1, NO, NO, crate::bytes::title),
    n!("isascii", BY, 1, 1, NO, NO, crate::bytes::isascii),
    n!("isalpha", BY, 1, 1, NO, NO, crate::bytes::isalpha),
    n!("isalnum", BY, 1, 1, NO, NO, crate::bytes::isalnum),
    n!("isdigit", BY, 1, 1, NO, NO, crate::bytes::isdigit),
    n!("isspace", BY, 1, 1, NO, NO, crate::bytes::isspace),
    n!("islower", BY, 1, 1, NO, NO, crate::bytes::islower),
    n!("isupper", BY, 1, 1, NO, NO, crate::bytes::isupper),
    n!("istitle", BY, 1, 1, NO, NO, crate::bytes::istitle),
    n!("center", BY, 2, 3, NO, NO, crate::bytes::center),
    n!("ljust", BY, 2, 3, NO, NO, crate::bytes::ljust),
    n!("rjust", BY, 2, 3, NO, NO, crate::bytes::rjust),
    n!("zfill", BY, 2, 2, NO, NO, crate::bytes::zfill),
    n!("removeprefix", BY, 2, 2, NO, NO, crate::bytes::removeprefix),
    n!("removesuffix", BY, 2, 2, NO, NO, crate::bytes::removesuffix),
    n!("partition", BY, 2, 2, NO, NO, crate::bytes::partition),
    n!("rpartition", BY, 2, 2, NO, NO, crate::bytes::rpartition),
    n!(
        "splitlines",
        BY,
        1,
        2,
        &["", "keepends"],
        NO,
        crate::bytes::splitlines
    ),
    // int
    n!(
        "bit_length",
        IN,
        1,
        1,
        NO,
        NO,
        crate::intops::int_bit_length
    ),
    n!("bit_count", IN, 1, 1, NO, NO, crate::intops::int_bit_count),
    n!("conjugate", IN, 1, 1, NO, NO, crate::intops::int_conjugate),
    n!(
        "is_integer",
        IN,
        1,
        1,
        NO,
        NO,
        crate::intops::int_is_integer
    ),
    n!(
        "as_integer_ratio",
        IN,
        1,
        1,
        NO,
        NO,
        crate::intops::int_as_integer_ratio
    ),
    n!(
        "to_bytes",
        IN,
        1,
        3,
        &["", "length", "byteorder"],
        &["signed"],
        crate::intops::int_to_bytes
    ),
    n!(
        "from_bytes",
        IN,
        2,
        3,
        &["", "bytes", "byteorder"],
        &["signed"],
        crate::intops::int_from_bytes
    ),
    // float
    n!(
        "is_integer",
        FL,
        1,
        1,
        NO,
        NO,
        crate::intops::float_is_integer
    ),
    n!(
        "conjugate",
        FL,
        1,
        1,
        NO,
        NO,
        crate::intops::float_conjugate
    ),
    n!(
        "as_integer_ratio",
        FL,
        1,
        1,
        NO,
        NO,
        crate::intops::float_as_integer_ratio
    ),
    n!("hex", FL, 1, 1, NO, NO, crate::intops::float_hex),
    n!("fromhex", FL, 2, 2, NO, NO, crate::intops::float_fromhex),
    // sys.stdout / sys.stderr
    n!("write", IO, 2, 2, NO, NO, stream_write),
    n!("flush", IO, 1, 1, NO, NO, stream_flush),
    // asyncio (host futures keep `done`, `result`, `exception`, `cancel`
    // and `cancelled` without it)
    #[cfg(feature = "asyncio")]
    m!(
        "asyncio",
        "gather",
        0,
        VAR,
        NO,
        &["return_exceptions"],
        aio_gather
    ),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_run", 2, 3, NO, NO, aio_run),
    #[cfg(feature = "asyncio")]
    m!(
        "asyncio",
        "create_task",
        1,
        1,
        &["coro"],
        &["name", "context"],
        aio_create_task
    ),
    #[cfg(feature = "asyncio")]
    n!("done", TK, 1, 1, NO, NO, aio_done),
    #[cfg(feature = "asyncio")]
    n!("result", TK, 1, 1, NO, NO, aio_result),
    #[cfg(feature = "asyncio")]
    n!("exception", TK, 1, 1, NO, NO, aio_exception),
    #[cfg(feature = "asyncio")]
    n!("cancel", TK, 1, 2, &["self", "msg"], NO, aio_cancel),
    #[cfg(feature = "asyncio")]
    n!("cancelled", TK, 1, 1, NO, NO, aio_cancelled),
    #[cfg(feature = "asyncio")]
    n!("cancelling", TK, 1, 1, NO, NO, aio_cancelling),
    #[cfg(feature = "asyncio")]
    n!("uncancel", TK, 1, 1, NO, NO, aio_uncancel),
    #[cfg(feature = "asyncio")]
    n!("get_name", TK, 1, 1, NO, NO, aio_get_name),
    #[cfg(feature = "asyncio")]
    n!("set_name", TK, 2, 2, &["self", "value"], NO, aio_set_name),
    #[cfg(feature = "asyncio")]
    n!("get_coro", TK, 1, 1, NO, NO, aio_get_coro),
    #[cfg(feature = "asyncio")]
    n!("get_loop", TK, 1, 1, NO, NO, aio_get_loop),
    #[cfg(feature = "asyncio")]
    n!(
        "add_done_callback",
        TK,
        2,
        2,
        &["self", "fn"],
        &["context"],
        aio_add_cb
    ),
    #[cfg(feature = "asyncio")]
    n!(
        "remove_done_callback",
        TK,
        2,
        2,
        &["self", "fn"],
        NO,
        aio_remove_cb
    ),
    #[cfg(feature = "asyncio")]
    n!("get_loop", FU, 1, 1, NO, NO, aio_get_loop),
    #[cfg(feature = "asyncio")]
    n!(
        "add_done_callback",
        FU,
        2,
        2,
        &["self", "fn"],
        &["context"],
        aio_add_cb
    ),
    #[cfg(feature = "asyncio")]
    n!(
        "remove_done_callback",
        FU,
        2,
        2,
        &["self", "fn"],
        NO,
        aio_remove_cb
    ),
    #[cfg(feature = "asyncio")]
    n!(
        "set_result",
        FU,
        2,
        2,
        &["self", "result"],
        NO,
        aio_set_result
    ),
    #[cfg(feature = "asyncio")]
    n!(
        "set_exception",
        FU,
        2,
        2,
        &["self", "exception"],
        NO,
        aio_set_exception
    ),
    #[cfg(feature = "asyncio")]
    n!("get_loop", GA, 1, 1, NO, NO, aio_get_loop),
    #[cfg(feature = "asyncio")]
    n!(
        "add_done_callback",
        GA,
        2,
        2,
        &["self", "fn"],
        &["context"],
        aio_add_cb
    ),
    #[cfg(feature = "asyncio")]
    n!(
        "remove_done_callback",
        GA,
        2,
        2,
        &["self", "fn"],
        NO,
        aio_remove_cb
    ),
    n!("done", FU, 1, 1, NO, NO, aio_done),
    n!("result", FU, 1, 1, NO, NO, aio_result),
    n!("exception", FU, 1, 1, NO, NO, aio_exception),
    n!("cancel", FU, 1, 2, &["self", "msg"], NO, aio_cancel),
    n!("cancelled", FU, 1, 1, NO, NO, aio_cancelled),
    #[cfg(feature = "asyncio")]
    n!("done", GA, 1, 1, NO, NO, aio_done),
    #[cfg(feature = "asyncio")]
    n!("result", GA, 1, 1, NO, NO, aio_result),
    #[cfg(feature = "asyncio")]
    n!("exception", GA, 1, 1, NO, NO, aio_exception),
    #[cfg(feature = "asyncio")]
    n!("cancel", GA, 1, 2, &["self", "msg"], NO, aio_cancel),
    #[cfg(feature = "asyncio")]
    n!("cancelled", GA, 1, 1, NO, NO, aio_cancelled),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_spawn", 1, 1, NO, NO, aio_create_task),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_first", 1, 1, NO, NO, aio_first),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_deadline", 1, 1, NO, NO, aio_deadline),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_future", 0, 0, NO, NO, aio_future),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_set", 2, 2, NO, NO, aio_set),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_current_task", 0, 0, NO, NO, aio_current_task),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_all_tasks", 0, 0, NO, NO, aio_all_tasks),
    #[cfg(feature = "asyncio")]
    m!(
        "_mm",
        "aio_spawn_internal",
        1,
        1,
        NO,
        NO,
        aio_spawn_internal
    ),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_name_set", 2, 2, NO, NO, aio_name_set),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_soon", 1, 1, NO, NO, aio_soon),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_time", 0, 0, NO, NO, aio_time),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_running", 0, 0, NO, NO, aio_running),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_set_exc", 2, 2, NO, NO, aio_set_exc),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_tasks", 0, 0, NO, NO, aio_tasks),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_stop", 1, 1, NO, NO, aio_stop),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_main_end", 2, 2, NO, NO, aio_main_end),
    #[cfg(feature = "asyncio")]
    m!(
        "asyncio",
        "sleep",
        1,
        2,
        &["delay", "result"],
        NO,
        aio_sleep
    ),
    m!("cmath", "acos", 1, 1, NO, NO, crate::cmath::c_acos_),
    m!("cmath", "acosh", 1, 1, NO, NO, crate::cmath::c_acosh_),
    m!("cmath", "asin", 1, 1, NO, NO, crate::cmath::c_asin_),
    m!("cmath", "asinh", 1, 1, NO, NO, crate::cmath::c_asinh_),
    m!("cmath", "atan", 1, 1, NO, NO, crate::cmath::c_atan_),
    m!("cmath", "atanh", 1, 1, NO, NO, crate::cmath::c_atanh_),
    m!("cmath", "cos", 1, 1, NO, NO, crate::cmath::c_cos_),
    m!("cmath", "cosh", 1, 1, NO, NO, crate::cmath::c_cosh_),
    m!("cmath", "exp", 1, 1, NO, NO, crate::cmath::c_exp_),
    m!("cmath", "log10", 1, 1, NO, NO, crate::cmath::c_log10_),
    m!("cmath", "sin", 1, 1, NO, NO, crate::cmath::c_sin_),
    m!("cmath", "sinh", 1, 1, NO, NO, crate::cmath::c_sinh_),
    m!("cmath", "sqrt", 1, 1, NO, NO, crate::cmath::c_sqrt_),
    m!("cmath", "tan", 1, 1, NO, NO, crate::cmath::c_tan_),
    m!("cmath", "tanh", 1, 1, NO, NO, crate::cmath::c_tanh_),
    m!("cmath", "phase", 1, 1, NO, NO, crate::cmath::c_phase),
    m!("cmath", "polar", 1, 1, NO, NO, crate::cmath::c_polar),
    m!("cmath", "isfinite", 1, 1, NO, NO, crate::cmath::c_isfinite),
    m!("cmath", "isinf", 1, 1, NO, NO, crate::cmath::c_isinf),
    m!("cmath", "isnan", 1, 1, NO, NO, crate::cmath::c_isnan),
    m!("cmath", "log", 1, 2, NO, NO, crate::cmath::c_log_),
    m!("cmath", "rect", 2, 2, NO, NO, crate::cmath::c_rect),
    m!(
        "cmath",
        "isclose",
        2,
        2,
        &["a", "b"],
        &["rel_tol", "abs_tol"],
        crate::cmath::c_isclose
    ),
    m!("math", "exp", 1, 1, NO, NO, crate::math::m_exp),
    m!("math", "expm1", 1, 1, NO, NO, crate::math::m_expm1),
    m!("math", "atan", 1, 1, NO, NO, crate::math::m_atan),
    m!("math", "sinh", 1, 1, NO, NO, crate::math::m_sinh),
    m!("math", "cosh", 1, 1, NO, NO, crate::math::m_cosh),
    m!("math", "tanh", 1, 1, NO, NO, crate::math::m_tanh),
    m!("math", "asinh", 1, 1, NO, NO, crate::math::m_asinh),
    m!("math", "erf", 1, 1, NO, NO, crate::math::m_erf),
    m!("math", "erfc", 1, 1, NO, NO, crate::math::m_erfc),
    m!("math", "cbrt", 1, 1, NO, NO, crate::math::m_cbrt),
    m!("math", "fabs", 1, 1, NO, NO, crate::math::m_fabs),
    m!("math", "degrees", 1, 1, NO, NO, crate::math::m_degrees),
    m!("math", "radians", 1, 1, NO, NO, crate::math::m_radians),
    m!("math", "exp2", 1, 1, NO, NO, crate::math::m_exp2),
    m!("math", "sin", 1, 1, NO, NO, crate::math::m_sin),
    m!("math", "cos", 1, 1, NO, NO, crate::math::m_cos),
    m!("math", "tan", 1, 1, NO, NO, crate::math::m_tan),
    m!("math", "asin", 1, 1, NO, NO, crate::math::m_asin),
    m!("math", "acos", 1, 1, NO, NO, crate::math::m_acos),
    m!("math", "acosh", 1, 1, NO, NO, crate::math::m_acosh),
    m!("math", "atanh", 1, 1, NO, NO, crate::math::m_atanh),
    m!("math", "sqrt", 1, 1, NO, NO, crate::math::m_sqrt),
    m!("math", "log2", 1, 1, NO, NO, crate::math::m_log2),
    m!("math", "log10", 1, 1, NO, NO, crate::math::m_log10),
    m!("math", "log1p", 1, 1, NO, NO, crate::math::m_log1p),
    m!("math", "isnan", 1, 1, NO, NO, crate::math::m_isnan),
    m!("math", "isinf", 1, 1, NO, NO, crate::math::m_isinf),
    m!("math", "isfinite", 1, 1, NO, NO, crate::math::m_isfinite),
    m!("math", "floor", 1, 1, NO, NO, crate::math::m_floor),
    m!("math", "ceil", 1, 1, NO, NO, crate::math::m_ceil),
    m!("math", "trunc", 1, 1, NO, NO, crate::math::m_trunc),
    m!("math", "modf", 1, 1, NO, NO, crate::math::m_modf),
    m!("math", "frexp", 1, 1, NO, NO, crate::math::m_frexp),
    m!("math", "gamma", 1, 1, NO, NO, crate::math::m_gamma),
    m!("math", "lgamma", 1, 1, NO, NO, crate::math::m_lgamma),
    m!("math", "ulp", 1, 1, NO, NO, crate::math::m_ulp),
    m!("math", "factorial", 1, 1, NO, NO, crate::math::m_factorial),
    m!("math", "isqrt", 1, 1, NO, NO, crate::math::m_isqrt),
    m!("math", "atan2", 2, 2, NO, NO, crate::math::m_atan2),
    m!("math", "pow", 2, 2, NO, NO, crate::math::m_pow),
    m!("math", "fmod", 2, 2, NO, NO, crate::math::m_fmod),
    m!("math", "remainder", 2, 2, NO, NO, crate::math::m_remainder),
    m!("math", "copysign", 2, 2, NO, NO, crate::math::m_copysign),
    m!("math", "fma", 3, 3, NO, NO, crate::math::m_fma),
    m!("math", "ldexp", 2, 2, NO, NO, crate::math::m_ldexp),
    m!("math", "nextafter", 2, 2, NO, NO, crate::math::m_nextafter),
    m!("math", "comb", 2, 2, NO, NO, crate::math::m_comb),
    m!("math", "log", 1, 2, NO, NO, crate::math::m_log),
    m!("math", "perm", 1, 2, NO, NO, crate::math::m_perm),
    m!("math", "gcd", 0, VAR, NO, NO, crate::math::m_gcd),
    m!("math", "lcm", 0, VAR, NO, NO, crate::math::m_lcm),
    m!("math", "fsum", 1, 1, NO, NO, crate::math::m_fsum),
    m!("math", "prod", 1, 1, NO, &["start"], crate::math::m_prod),
    m!("math", "sumprod", 2, 2, NO, NO, crate::math::m_sumprod),
    m!("math", "hypot", 0, VAR, NO, NO, crate::math::m_hypot),
    m!("math", "dist", 2, 2, NO, NO, crate::math::m_dist),
    m!(
        "math",
        "isclose",
        2,
        2,
        &["a", "b"],
        &["rel_tol", "abs_tol"],
        crate::math::m_isclose
    ),
    m!(
        "sys",
        "setrecursionlimit",
        1,
        1,
        NO,
        NO,
        sys_setrecursionlimit
    ),
    m!(
        "sys",
        "getrecursionlimit",
        0,
        0,
        NO,
        NO,
        sys_getrecursionlimit
    ),
    m!("sys", "exception", 0, 0, NO, NO, sys_exception),
    m!("sys", "exc_info", 0, 0, NO, NO, sys_exc_info),
    m!("traceback", "_extract", 2, 2, NO, NO, tb_extract),
    m!("traceback", "_hint", 2, 2, NO, NO, tb_hint),
    #[cfg(feature = "ast")]
    m!("ast", "_parse", 4, 4, NO, NO, crate::astmod::ast_parse),
    #[cfg(feature = "tokenize")]
    m!("tokenize", "_tokens", 1, 1, NO, NO, crate::tokmod::tokens),
    m!("_mm", "signature", 7, 7, NO, NO, crate::mmmod::mm_signature),
    m!("_mm", "backing", 1, 1, NO, NO, crate::mmmod::mm_backing),
    // Also `method`'s constructor (see `ctor`).
    m!("_mm", "method", 2, 2, NO, NO, crate::mmmod::mm_method),
    m!("_mm", "clock", 0, 0, NO, NO, crate::mmmod::mm_clock),
    m!("_mm", "tzif", 1, 1, NO, NO, crate::mmmod::mm_tzif),
    m!("_mm", "caller", 2, 2, NO, NO, crate::mmmod::mm_caller),
    m!(
        "_mm",
        "exc_same_meta",
        2,
        2,
        NO,
        NO,
        crate::mmmod::mm_exc_same_meta
    ),
    m!(
        "_mm",
        "exc_tb_here",
        1,
        1,
        NO,
        NO,
        crate::mmmod::mm_exc_tb_here
    ),
    m!("_mm", "ctx_get", 0, 0, NO, NO, mm_ctx_get),
    m!("_mm", "ctx_swap", 1, 1, NO, NO, mm_ctx_swap),
    m!("_mm", "ctx_copy", 0, 0, NO, NO, mm_ctx_copy),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_task_ctx", 2, 2, NO, NO, aio_task_ctx),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_unretrieved", 0, 0, NO, NO, aio_unretrieved),
    #[cfg(feature = "asyncio")]
    m!("_mm", "aio_escape", 0, 0, NO, NO, aio_escape),
    m!("_mm", "code_kind", 1, 1, NO, NO, crate::mmmod::mm_code_kind),
    m!(
        "_mm",
        "local_zone",
        0,
        0,
        NO,
        NO,
        crate::mmmod::mm_local_zone
    ),
    // A builtin function in CPython; the work is itertools.py's `_tee_py`.
    m!("itertools", "tee", 1, 2, NO, NO, to_module_twin),
    m!(
        "_mm",
        "gc_collect",
        0,
        0,
        NO,
        NO,
        crate::mmmod::mm_gc_collect
    ),
    m!("_mm", "gc_wait", 0, 0, NO, NO, crate::mmmod::mm_gc_wait),
    m!("_mm", "urandom", 1, 1, NO, NO, crate::mmmod::mm_urandom),
    m!("_mm", "hint", 1, 1, NO, NO, crate::hints::mm_hint),
    m!("_mm", "host_help", 1, 1, NO, NO, crate::host::mm_host_help),
    m!(
        "_mm",
        "os_function",
        1,
        1,
        NO,
        NO,
        crate::mmmod::mm_os_function
    ),
    #[cfg(feature = "datetime")]
    m!("_mm", "dt_iso", 2, 2, NO, NO, crate::datetimemod::dt_iso),
    #[cfg(feature = "datetime")]
    m!("_mm", "dt_ord", 3, 3, NO, NO, crate::datetimemod::dt_ord),
    #[cfg(feature = "datetime")]
    m!("_mm", "dt_ymd", 1, 1, NO, NO, crate::datetimemod::dt_ymd),
    #[cfg(feature = "datetime")]
    m!(
        "_mm",
        "dt_strptime",
        2,
        2,
        NO,
        NO,
        crate::datetimemod::dt_strptime
    ),
    #[cfg(feature = "struct")]
    m!(
        "struct",
        "_calcsize",
        1,
        1,
        NO,
        NO,
        crate::structmod::calcsize
    ),
    #[cfg(feature = "struct")]
    m!("struct", "_pack", 2, 2, NO, NO, crate::structmod::pack),
    #[cfg(feature = "struct")]
    m!(
        "struct",
        "_calclen",
        1,
        1,
        NO,
        NO,
        crate::structmod::calclen
    ),
    #[cfg(feature = "struct")]
    m!("struct", "_unpack", 4, 4, NO, NO, crate::structmod::unpack),
    #[cfg(feature = "binascii")]
    m!(
        "binascii",
        "_b2a_base64",
        2,
        2,
        NO,
        NO,
        crate::binascii::b2a_base64
    ),
    #[cfg(feature = "binascii")]
    m!(
        "binascii",
        "_a2b_base64",
        2,
        2,
        NO,
        NO,
        crate::binascii::a2b_base64
    ),
    #[cfg(feature = "binascii")]
    m!(
        "binascii",
        "_hexlify",
        1,
        1,
        NO,
        NO,
        crate::binascii::hexlify
    ),
    #[cfg(feature = "binascii")]
    m!(
        "binascii",
        "_unhexlify",
        1,
        1,
        NO,
        NO,
        crate::binascii::unhexlify
    ),
    #[cfg(feature = "unicodedata")]
    m!(
        "unicodedata",
        "category",
        1,
        1,
        NO,
        NO,
        crate::unicodedata::category
    ),
    #[cfg(feature = "unicodedata")]
    m!(
        "unicodedata",
        "combining",
        1,
        1,
        NO,
        NO,
        crate::unicodedata::combining
    ),
    #[cfg(feature = "unicodedata")]
    m!(
        "unicodedata",
        "decomposition",
        1,
        1,
        NO,
        NO,
        crate::unicodedata::decomposition
    ),
    #[cfg(feature = "unicodedata")]
    m!(
        "unicodedata",
        "decimal",
        1,
        2,
        NO,
        NO,
        crate::unicodedata::decimal
    ),
    #[cfg(feature = "unicodedata")]
    m!(
        "unicodedata",
        "digit",
        1,
        2,
        NO,
        NO,
        crate::unicodedata::digit
    ),
    #[cfg(feature = "unicodedata")]
    m!(
        "unicodedata",
        "numeric",
        1,
        2,
        NO,
        NO,
        crate::unicodedata::numeric
    ),
    #[cfg(feature = "unicodedata")]
    m!(
        "unicodedata",
        "name",
        1,
        2,
        NO,
        NO,
        crate::unicodedata::name
    ),
    #[cfg(feature = "unicodedata")]
    m!(
        "unicodedata",
        "lookup",
        1,
        1,
        NO,
        NO,
        crate::unicodedata::lookup
    ),
    #[cfg(feature = "unicodedata")]
    m!(
        "unicodedata",
        "normalize",
        2,
        2,
        NO,
        NO,
        crate::unicodedata::normalize
    ),
    #[cfg(feature = "unicodedata")]
    m!(
        "unicodedata",
        "is_normalized",
        2,
        2,
        NO,
        NO,
        crate::unicodedata::is_normalized
    ),
    #[cfg(feature = "binascii")]
    m!("binascii", "_crc32", 2, 2, NO, NO, crate::binascii::crc32),
    #[cfg(feature = "binascii")]
    m!(
        "binascii",
        "_adler32",
        2,
        2,
        NO,
        NO,
        crate::binascii::adler32
    ),
    #[cfg(feature = "binascii")]
    m!(
        "binascii",
        "_crc_hqx",
        2,
        2,
        NO,
        NO,
        crate::binascii::crc_hqx
    ),
    #[cfg(feature = "hashlib")]
    m!("hashlib", "_h_new", 1, 3, NO, NO, crate::hashlib::h_new),
    #[cfg(feature = "hashlib")]
    m!(
        "hashlib",
        "_h_update",
        2,
        2,
        NO,
        NO,
        crate::hashlib::h_update
    ),
    #[cfg(feature = "hashlib")]
    m!("hashlib", "_h_copy", 1, 1, NO, NO, crate::hashlib::h_copy),
    #[cfg(feature = "hashlib")]
    m!(
        "hashlib",
        "_h_digest",
        2,
        2,
        NO,
        NO,
        crate::hashlib::h_digest
    ),
    #[cfg(feature = "hashlib")]
    m!(
        "hashlib",
        "_h_pbkdf2",
        5,
        5,
        NO,
        NO,
        crate::hashlib::h_pbkdf2
    ),
    #[cfg(feature = "random")]
    m!("random", "_mt_new", 0, 0, NO, NO, crate::random::mt_new),
    #[cfg(feature = "random")]
    m!("random", "_mt_seed", 2, 2, NO, NO, crate::random::mt_seed),
    #[cfg(feature = "random")]
    m!(
        "random",
        "_mt_random",
        1,
        1,
        NO,
        NO,
        crate::random::mt_random
    ),
    #[cfg(feature = "random")]
    m!("random", "_mt_bits", 2, 2, NO, NO, crate::random::mt_bits),
    #[cfg(feature = "random")]
    m!("random", "_mt_below", 2, 2, NO, NO, crate::random::mt_below),
    #[cfg(feature = "random")]
    m!(
        "random",
        "_mt_getstate",
        1,
        1,
        NO,
        NO,
        crate::random::mt_getstate
    ),
    #[cfg(feature = "random")]
    m!(
        "random",
        "_mt_setstate",
        2,
        2,
        NO,
        NO,
        crate::random::mt_setstate
    ),
    #[cfg(feature = "re")]
    m!("re", "_exec", 7, 7, NO, NO, crate::re::re_exec),
    #[cfg(feature = "re")]
    m!("re", "_pack", 1, 1, NO, NO, crate::re::re_pack),
    #[cfg(feature = "re")]
    m!("re", "_fold", 2, 2, NO, NO, crate::re::re_fold),
    #[cfg(feature = "re")]
    m!("re", "_match", 6, 6, NO, NO, crate::re::re_match),
    #[cfg(feature = "re")]
    n!("group", MA, 1, VAR, NO, NO, crate::re::match_group),
    #[cfg(feature = "re")]
    n!("__getitem__", MA, 2, 2, NO, NO, crate::re::match_getitem),
    #[cfg(feature = "re")]
    n!(
        "groups",
        MA,
        1,
        2,
        &["", "default"],
        NO,
        crate::re::match_groups
    ),
    #[cfg(feature = "re")]
    n!(
        "groupdict",
        MA,
        1,
        2,
        &["", "default"],
        NO,
        crate::re::match_groupdict
    ),
    #[cfg(feature = "re")]
    n!("start", MA, 1, 2, NO, NO, crate::re::match_start),
    #[cfg(feature = "re")]
    n!("end", MA, 1, 2, NO, NO, crate::re::match_end),
    #[cfg(feature = "re")]
    n!("span", MA, 1, 2, NO, NO, crate::re::match_span),
    #[cfg(feature = "re")]
    n!(
        "expand",
        MA,
        2,
        2,
        &["", "template"],
        NO,
        crate::re::match_expand
    ),
    #[cfg(feature = "re")]
    n!("__copy__", MA, 1, 1, NO, NO, crate::re::match_copy),
    #[cfg(feature = "re")]
    n!("__deepcopy__", MA, 2, 2, NO, NO, crate::re::match_copy),
    p!("_dict_probe", 4, 4, pl_dict_probe),
    p!("_pct", 3, 3, crate::percent::pl_pct),
    #[cfg(feature = "json")]
    p!("_json_encode", 10, 10, crate::json::pl_json_encode),
    #[cfg(feature = "json")]
    p!("_json_str", 2, 2, crate::json::pl_json_str),
    #[cfg(feature = "json")]
    p!("_json_float", 2, 2, crate::json::pl_json_float),
    #[cfg(feature = "json")]
    p!("_json_scan", 1, 4, crate::json::pl_json_scan),
    p!("_dict_insert", 4, 4, pl_dict_insert),
    p!("_dict_value_at", 2, 2, pl_dict_value_at),
    p!("_dict_set_at", 3, 3, pl_dict_set_at),
    p!("_dict_at", 2, 2, pl_dict_at),
    p!("_dict_del_at", 2, 2, pl_dict_del_at),
    p!("_set_probe", 5, 5, pl_set_probe),
    p!("_seq_scan", 4, 4, pl_seq_scan),
    p!("_more", 1, 4, pl_more),
    p!("_dict_scan", 4, 4, pl_dict_scan),
    p!("_walk_eq", 3, 3, pl_walk_eq),
    p!("_walk_ord", 3, 3, pl_walk_ord),
    p!("_walk_repr", 3, 3, pl_walk_repr),
    p!("_set_insert", 3, 3, pl_set_insert),
    p!("_set_table", 2, 2, pl_set_table),
    p!("_set_hashed_at", 2, 2, pl_set_hashed),
    p!("_cell_set", 2, 2, pl_cell_set),
    p!("_build_class", 5, 5, pl_build_class),
    // A builtin bound on its first lookup (`prelude::AS_BUILTINS`), so
    // startup does not pay for it.
    m!(
        "_prelude",
        "__import__",
        1,
        5,
        &["name", "globals", "locals", "fromlist", "level"],
        NO,
        bi_import
    ),
    p!("_set_exc_str", 2, 2, pl_set_exc_str),
    p!("_hash_tuple", 4, 4, pl_hash_tuple),
    p!("_list_replace", 2, 2, pl_list_replace),
    p!("_lazy", 1, 2, pl_lazy),
    p!("_native_name", 1, 1, pl_native_name),
    p!("_type_name", 1, 1, pl_type_name),
    p!("_cmp_error", 3, 3, pl_cmp_error),
    p!("_bx_base", 1, 1, pl_bx_base),
    p!("_bx_init", 1, 1, pl_bx_init),
    p!("_bx_make", 2, 2, pl_bx_make),
    p!("_bx_value", 1, 1, crate::mmmod::mm_backing),
];

// ------------------------------------------------------- packed table

/// Every builtin, functions first in table order, then each builtin type's
/// methods as one run ([`OWNER_START`]) sorted by name length, then bytes.
/// A `NativeFn` value is an index here.
pub static NATIVES: &[Native] = &NATIVE_ARRAY;

/// [`NATIVES`]' own static: borrowing `PACKED.natives` directly would
/// ship the whole const `Packed`, string slots and blob at capacity
/// included (about 33 KB more).
static NATIVE_ARRAY: [Native; COUNT] = PACKED.natives;

/// Every name in [`NATIVES`], each stored once.
static BLOB: &str = match core::str::from_utf8(&BLOB_BYTES) {
    Ok(s) => s,
    Err(_) => "",
};
const BLOB_BYTES: [u8; PACKED.blob_len] = prefix(&PACKED.blob);

/// The parameter-name runs [`Names`] index.
static LISTS: [Sref; PACKED.lists_len] = prefix(&PACKED.lists);

/// The bundled modules natives belong to.
static MODULES: [Sref; PACKED.modules_len] = prefix(&PACKED.modules);

/// `NATIVES[OWNER_START[t]..OWNER_START[t + 1]]` are type `t`'s methods.
static OWNER_START: [u16; crate::types::NTYPES + 1] = PACKED.owner_start;

const COUNT: usize = DEFS.len();
const STR_CAP: usize = str_cap();
const LIST_CAP: usize = list_cap();

const fn str_cap() -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < COUNT {
        let d = &DEFS[i];
        n += d.name.len() + 1;
        if let Some(m) = d.module {
            n += m.len();
        }
        let mut j = 0;
        while j < d.params.len() {
            n += d.params[j].len();
            j += 1;
        }
        j = 0;
        while j < d.kwonly.len() {
            n += d.kwonly[j].len();
            j += 1;
        }
        i += 1;
    }
    n
}

const fn list_cap() -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < COUNT {
        n += DEFS[i].params.len() + DEFS[i].kwonly.len();
        i += 1;
    }
    n
}

struct Packed {
    natives: [Native; COUNT],
    blob: [u8; STR_CAP],
    blob_len: usize,
    lists: [Sref; LIST_CAP],
    lists_len: usize,
    modules: [Sref; 64],
    modules_len: usize,
    strs: [Sref; STR_CAP],
    strs_len: usize,
    owner_start: [u16; crate::types::NTYPES + 1],
}

#[allow(
    clippy::large_const_arrays,
    reason = "compile-time only: the statics above copy out what ships"
)]
const PACKED: Packed = pack();

const fn prefix<E: Copy, const N: usize, const M: usize>(a: &[E; M]) -> [E; N] {
    let mut out = [a[0]; N];
    let mut i = 0;
    while i < N {
        out[i] = a[i];
        i += 1;
    }
    out
}

/// The order of [`NATIVES`]: `a` (a method) before `b`?
const fn method_before(a: &Def, b: &Def) -> bool {
    let (Some(ta), Some(tb)) = (a.owner, b.owner) else {
        return false;
    };
    if ta as u8 != tb as u8 {
        return (ta as u8) < (tb as u8);
    }
    name_before(a.name.as_bytes(), b.name.as_bytes())
}

/// Shorter names first, then bytewise, as [`find_method`] searches.
const fn name_before(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return a.len() < b.len();
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
        i += 1;
    }
    false
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the blob, the lists and the table are checked to fit below"
)]
const fn add_str(p: &mut Packed, s: &str) -> Sref {
    let b = s.as_bytes();
    let mut k = 0;
    while k < p.strs_len {
        let r = p.strs[k];
        if r.len as usize == b.len() {
            let at = r.off as usize;
            let mut j = 0;
            while j < b.len() && p.blob[at + j] == b[j] {
                j += 1;
            }
            if j == b.len() {
                return r;
            }
        }
        k += 1;
    }
    let off = p.blob_len;
    let mut j = 0;
    while j < b.len() {
        p.blob[off + j] = b[j];
        j += 1;
    }
    p.blob_len += b.len();
    assert!(p.blob_len <= u16::MAX as usize && b.len() <= u8::MAX as usize);
    let r = Sref {
        off: off as u16,
        len: b.len() as u8,
    };
    p.strs[p.strs_len] = r;
    p.strs_len += 1;
    r
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the blob, the lists and the table are checked to fit below"
)]
const fn add_list(p: &mut Packed, a: &[&str], b: &[&str]) -> u16 {
    let mut refs = [Sref { off: 0, len: 0 }; 16];
    let n = a.len() + b.len();
    assert!(n <= refs.len());
    let mut k = 0;
    while k < n {
        refs[k] = add_str(p, if k < a.len() { a[k] } else { b[k - a.len()] });
        k += 1;
    }
    let mut at = 0;
    while n > 0 && at + n <= p.lists_len {
        let mut j = 0;
        while j < n && p.lists[at + j].off == refs[j].off && p.lists[at + j].len == refs[j].len {
            j += 1;
        }
        if j == n {
            return at as u16;
        }
        at += 1;
    }
    let start = p.lists_len;
    let mut j = 0;
    while j < n {
        p.lists[start + j] = refs[j];
        j += 1;
    }
    p.lists_len += n;
    start as u16
}

/// 1 + the index of module `m` in [`MODULES`].
#[allow(clippy::cast_possible_truncation, reason = "checked to fit")]
const fn add_module(p: &mut Packed, m: &str) -> u8 {
    let r = add_str(p, m);
    let mut i = 0;
    while i < p.modules_len {
        if p.modules[i].off == r.off && p.modules[i].len == r.len {
            return (i + 1) as u8;
        }
        i += 1;
    }
    p.modules[i] = r;
    p.modules_len += 1;
    assert!(p.modules_len < u8::MAX as usize);
    (i + 1) as u8
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the blob, the lists and the table are checked to fit below"
)]
#[allow(clippy::large_stack_arrays, reason = "evaluated at compile time only")]
const fn pack() -> Packed {
    const fn stub(_: &mut Vm, _: CallArgs) -> Result<Value, Raised> {
        Err(Raised)
    }
    let none = Sref { off: 0, len: 0 };
    let blank = Native {
        owner: None,
        min: 0,
        max: 0,
        module: 0,
        name: none,
        args: 0,
        nparams: 0,
        nkwonly: 0,
        f: stub,
    };
    let mut p = Packed {
        natives: [blank; COUNT],
        blob: [0; STR_CAP],
        blob_len: 0,
        lists: [none; LIST_CAP],
        lists_len: 0,
        modules: [none; 64],
        modules_len: 0,
        strs: [none; STR_CAP],
        strs_len: 0,
        owner_start: [0; crate::types::NTYPES + 1],
    };
    // The order: functions as written, then methods (insertion sort).
    let mut order = [0u16; COUNT];
    let mut n = 0;
    let mut i = 0;
    while i < COUNT {
        if DEFS[i].owner.is_none() {
            order[n] = i as u16;
            n += 1;
        }
        i += 1;
    }
    let functions = n;
    i = 0;
    while i < COUNT {
        if DEFS[i].owner.is_some() {
            let mut j = n;
            while j > functions && method_before(&DEFS[i], &DEFS[order[j - 1] as usize]) {
                order[j] = order[j - 1];
                j -= 1;
            }
            assert!(
                j == functions || method_before(&DEFS[order[j - 1] as usize], &DEFS[i]),
                "two methods of one type share a name"
            );
            order[j] = i as u16;
            n += 1;
        }
        i += 1;
    }
    assert!(COUNT <= u16::MAX as usize);
    let mut k = 0;
    while k < COUNT {
        let d = &DEFS[order[k] as usize];
        let name = add_str(&mut p, d.name);
        let module = match d.module {
            Some(m) => add_module(&mut p, m),
            None => 0,
        };
        let args = add_list(&mut p, d.params, d.kwonly);
        p.natives[k] = Native {
            owner: d.owner,
            min: d.min,
            max: d.max,
            module,
            name,
            args,
            nparams: d.params.len() as u8,
            nkwonly: d.kwonly.len() as u8,
            f: d.f,
        };
        k += 1;
    }
    // Each type's run of methods.
    let mut t = 0;
    let mut k = functions;
    while t <= crate::types::NTYPES {
        while k < COUNT
            && match DEFS[order[k] as usize].owner {
                Some(o) => (o as usize) < t,
                None => false,
            }
        {
            k += 1;
        }
        p.owner_start[t] = k as u16;
        t += 1;
    }
    p
}

/// Is builtin method `n` a class method (bound to the type, also when
/// looked up on an instance)?
pub(crate) fn is_classmethod(n: &Native) -> bool {
    matches!(
        (n.owner, n.name()),
        (Some(T::bytes | T::float), "fromhex")
            | (Some(T::str | T::bytes), "maketrans")
            | (Some(T::int), "from_bytes")
            | (Some(T::dict), "fromkeys")
            | (Some(T::complex), "from_number")
    )
}

/// The builtin with table index `i`.
#[must_use]
pub fn native(i: u32) -> Option<&'static Native> {
    NATIVES.get(i as usize)
}

/// The method `name` of builtin type `owner` (bool inherits int's).
#[must_use]
pub fn find_method(owner: T, name: &str) -> Option<u32> {
    method_index(owner, name.as_bytes())
}

/// [`find_method`] for a name's UTF-8 bytes: a binary search of the
/// type's run of [`NATIVES`].
#[inline(never)]
pub(crate) fn method_index(owner: T, name: &[u8]) -> Option<u32> {
    let t = owner as usize;
    let lo = *OWNER_START.get(t)? as usize;
    let hi = *OWNER_START.get(t + 1)? as usize;
    let at = NATIVES.get(lo..hi)?.binary_search_by(|n| {
        (n.name.len as usize)
            .cmp(&name.len())
            .then_with(|| n.name.get().as_bytes().cmp(name))
    });
    match at {
        Ok(k) => u32::try_from(lo + k).ok(),
        // bytearray shares bytes' read-only methods.
        Err(_) if owner == T::bytearray => method_index(T::bytes, name),
        // The builtin iterators share `callable_iterator`'s `__next__` and
        // `__iter__`.
        Err(_)
            if matches!(
                owner,
                T::list_iterator
                    | T::tuple_iterator
                    | T::str_iterator
                    | T::str_ascii_iterator
                    | T::bytes_iterator
                    | T::bytearray_iterator
                    | T::range_iterator
                    | T::dict_keyiterator
                    | T::dict_valueiterator
                    | T::dict_itemiterator
                    | T::set_iterator
                    | T::enumerate
                    | T::zip
                    | T::list_reverseiterator
            ) =>
        {
            method_index(T::callable_iterator, name)
        }
        Err(_) => None,
    }
}

/// The methods of builtin type `owner`.
pub(crate) fn methods_of(owner: T) -> &'static [Native] {
    let t = owner as usize;
    match (OWNER_START.get(t), OWNER_START.get(t + 1)) {
        (Some(&lo), Some(&hi)) => NATIVES.get(lo as usize..hi as usize).unwrap_or(&[]),
        _ => &[],
    }
}

/// `t.__dict__` for builtin type `t`: a new dict of its method-table
/// entries. CPython's is a read-only mappingproxy that also holds the
/// slot wrappers and data descriptors the table does not.
pub(crate) fn type_dict(vm: &mut Vm, owner: T) -> Result<Value, Raised> {
    let d = vm.new_dict()?;
    // bytearray's own run after the bytes methods it shares (`method_index`).
    let runs: &[T] = if owner == T::bytearray {
        &[T::bytes, T::bytearray]
    } else {
        core::slice::from_ref(&owner)
    };
    for &o in runs {
        let t = o as usize;
        if let (Some(&lo), Some(&hi)) = (OWNER_START.get(t), OWNER_START.get(t + 1)) {
            for i in u32::from(lo)..u32::from(hi) {
                if let Some(n) = native(i) {
                    let k = vm.intern(n.name())?;
                    vm.dict_set(d, k, Value::native(i))?;
                }
            }
        }
    }
    Ok(d)
}

/// The public attribute names CPython's `dir()` lists for a builtin type,
/// for "Did you mean" hints: its method table's public names, and the data
/// attributes the table does not hold.
#[must_use]
pub fn method_names(owner: T) -> Vec<&'static str> {
    let (table, data): (T, &[&str]) = match owner {
        T::str | T::list | T::dict | T::tuple | T::bytes | T::set | T::frozenset => (owner, &[]),
        T::int | T::bool => (T::int, &["denominator", "imag", "numerator", "real"]),
        T::float => (T::float, &["imag", "real"]),
        T::complex => (T::complex, &["imag", "real"]),
        T::BaseException => (T::BaseException, &["args"]),
        T::range => (T::range, &["count", "index", "start", "step", "stop"]),
        T::Match => (
            T::Match,
            &[
                "endpos",
                "lastgroup",
                "lastindex",
                "pos",
                "re",
                "regs",
                "string",
            ],
        ),
        _ => return Vec::new(),
    };
    let mut names: Vec<&'static str> = methods_of(table)
        .iter()
        .map(Native::name)
        .filter(|n| !n.starts_with('_'))
        .collect();
    names.extend_from_slice(data);
    names
}

/// Installs builtin names and type constructors.
pub(crate) fn install(vm: &mut Vm) -> Result<(), Raised> {
    let b = vm.builtins;
    for (i, n) in NATIVES.iter().enumerate() {
        if n.owner.is_some() || n.module != 0 {
            continue;
        }
        let name = n.name();
        let is_type = crate::types::ALL
            .iter()
            .any(|t| t.name() == name && t.is_builtin_name());
        if is_type {
            continue;
        }
        let k = vm.intern(name)?;
        vm.dict_set(b, k, Value::native(u32::try_from(i).unwrap_or(0)))?;
    }
    for t in crate::types::ALL {
        if t.is_builtin_name() {
            let k = vm.intern(t.name())?;
            let v = vm.type_obj(t);
            vm.dict_set(b, k, v)?;
        }
    }
    for name in ["IOError", "EnvironmentError"] {
        let k = vm.intern(name)?;
        let v = vm.type_obj(T::OSError);
        vm.dict_set(b, k, v)?;
    }
    for (name, v) in [
        ("Ellipsis", Value::ELLIPSIS),
        ("NotImplemented", Value::NOT_IMPLEMENTED),
        ("__debug__", Value::TRUE),
    ] {
        let k = vm.intern(name)?;
        vm.dict_set(b, k, v)?;
    }
    // Code run without a `__name__` global sees the builtins module's.
    let k = vm.intern("__name__")?;
    let v = vm.intern("builtins")?;
    vm.dict_set(b, k, v)?;
    Ok(())
}

/// The constructor native of builtin type `t`.
pub(crate) fn ctor(t: T) -> Option<u32> {
    NATIVES
        .iter()
        .position(|n| {
            n.owner.is_none()
                && n.name() == t.name()
                && (n.module().is_none() || n.module() == Some("_mm"))
        })
        .and_then(|i| u32::try_from(i).ok())
}

// ------------------------------------------------------------- helpers

impl Vm {
    /// Positional argument `i` (`UNDEFINED` if absent).
    #[inline]
    pub(crate) fn arg(&self, a: &CallArgs, i: u32) -> Value {
        if i < a.n {
            self.stack
                .get((a.base + i) as usize)
                .copied()
                .unwrap_or(Value::UNDEFINED)
        } else {
            Value::UNDEFINED
        }
    }

    /// [`Vm::arg`], out of line (see [`Vm::slot_o`]).
    #[inline(never)]
    pub(crate) fn arg_o(&self, a: &CallArgs, i: u32) -> Value {
        self.arg(a, i)
    }

    /// Keyword-only argument `j` (`UNDEFINED` if absent).
    #[inline]
    pub(crate) fn kwarg(&self, a: &CallArgs, j: u32) -> Value {
        if a.kw == u32::MAX {
            return Value::UNDEFINED;
        }
        self.stack
            .get((a.kw + j) as usize)
            .copied()
            .unwrap_or(Value::UNDEFINED)
    }

    fn args_vec(&self, a: &CallArgs) -> Vec<Value> {
        (0..a.n).map(|i| self.arg_o(a, i)).collect()
    }

    /// An index argument clamped to the `i64` range, like CPython's
    /// `PyNumber_AsSsize_t(o, NULL)`.
    pub(crate) fn ssize_arg(&mut self, v: Value) -> Result<i64, Raised> {
        let v = if self.uflags(v) & crate::class::f::INDEX == 0 {
            self.unbx(v)
        } else {
            v
        };
        if Vm::is_heap_int(v) && self.int_of_o(v).is_none() {
            let neg = self.big_of(v).is_some_and(|b| b.neg);
            return Ok(if neg { i64::MIN } else { i64::MAX });
        }
        self.int_arg(v, "")
    }

    /// An int argument, or `TypeError`.
    pub(crate) fn int_arg(&mut self, v: Value, _what: &str) -> Result<i64, Raised> {
        match self.int_of_o(v) {
            Some(i) => Ok(i),
            None if self.uflags(v) & crate::class::f::INDEX == 0
                && self.backing(v).is_some_and(Vm::is_intlike) =>
            {
                let b = self.unbx(v);
                self.int_arg(b, "")
            }
            None if Vm::is_heap_int(v) => Err(self.err(
                T::OverflowError,
                "Python int too large to convert to C ssize_t",
            )),
            None if self.uflags(v) & crate::class::f::INDEX != 0 => Err(self.need_py()),
            None => Err(self.err(
                T::TypeError,
                &format!(
                    "'{}' object cannot be interpreted as an integer",
                    self.type_name(v)
                ),
            )),
        }
    }

    /// Calls builtin `idx` after checking arity and binding keywords.
    /// Arguments: `npos` positional values from `base`, then one value per
    /// name in the `kwnames` tuple. `top` is free stack space.
    pub(crate) fn call_native(
        &mut self,
        idx: u32,
        base: u32,
        npos: u32,
        kwnames: Value,
        top: u32,
    ) -> Result<Value, Raised> {
        let Some(n) = native(idx) else {
            return Err(self.err(T::SystemError, "bad builtin"));
        };
        // A builtin type's method called on an instance of a subclass
        // works on the value the instance extends.
        if let Some(o) = n.owner
            && Vm::is_bx_type(o)
            && npos > 0
            && let Some(b) = self.backing(self.stack[base as usize])
            && !keeps_instance(n)
        {
            self.stack[base as usize] = b;
        }
        let nkw = if kwnames == Value::NONE {
            0
        } else {
            u32::try_from(self.tuple_len(kwnames)).unwrap_or(0)
        };
        let direct = core::mem::take(&mut self.direct_call);
        if nkw == 0 {
            self.check_arity(n, npos)?;
            let args = CallArgs {
                base,
                n: npos,
                kw: u32::MAX,
            };
            // Only the dispatch loop's own call re-runs the instruction.
            if direct && self.fuel.is_some() && self.native_short(n, &args) {
                return Err(Raised);
            }
            let r = (n.f)(self, args);
            if r.is_err() && self.slow {
                self.slow = false;
                return self.hand_to_twin(idx, n, args);
            }
            if r.is_err() && direct && self.oom_retry(n) {
                return Err(Raised);
            }
            return r;
        }
        if n.params().iter().all(str::is_empty) && n.kwonly().is_empty() {
            let q = qualname(n);
            // Dunders of builtin types are slot wrappers in CPython.
            let msg = if n.owner.is_some()
                && n.name().starts_with("__")
                && n.name().ends_with("__")
                && !matches!(n.name(), "__init__" | "__new__")
            {
                format!("wrapper {}() takes no keyword arguments", n.name())
            } else if let (None, Some(m)) = (n.owner, n.module()) {
                // Module functions are named with their module.
                format!("{m}.{q}() takes no keyword arguments")
            } else {
                format!("{q}() takes no keyword arguments")
            };
            return Err(self.err(T::TypeError, &msg));
        }
        // The argument clinic counts every argument first (a method's
        // receiver aside, and naming the method bare).
        let recv = u32::from(n.owner.is_some());
        let total = u32::from(n.max) + u32::try_from(n.kwonly().len()).unwrap_or(0) - recv;
        if n.max != VAR && !n.kwonly().contains("**") && npos + nkw > total + recv {
            let q = if recv == 1 {
                String::from(n.name())
            } else {
                qualname(n)
            };
            let msg = format!(
                // "keyword" when no positional was passed.
                "{q}() takes at most {total} {}argument{} ({} given)",
                if npos == recv { "keyword " } else { "" },
                if total == 1 { "" } else { "s" },
                npos + nkw - recv
            );
            return Err(self.err(T::TypeError, &msg));
        }
        if n.params().iter().all(str::is_empty) {
            // Positional-only parameters are counted before keywords match.
            self.check_arity(n, npos)?;
        }
        // Normalise into scratch space: positional, then keyword-only.
        let names = self.tuple_items(kwnames);
        let max_pos = if n.max == VAR {
            npos
        } else {
            u32::from(n.max).max(npos)
        };
        let nk = u32::try_from(n.kwonly().len()).unwrap_or(0);
        let need = (top + max_pos + nk) as usize + 1;
        self.ensure_stack(need)?;
        for i in 0..max_pos + nk {
            self.stack[(top + i) as usize] = Value::UNDEFINED;
        }
        for i in 0..npos {
            self.stack[(top + i) as usize] = self.stack[(base + i) as usize];
        }
        let mut npos_out = npos;
        let mut extra: Option<Value> = None;
        for (k, name) in names.iter().enumerate() {
            let val = self.stack[(base + npos + u32::try_from(k).unwrap_or(0)) as usize];
            let key = String::from(self.str_ref(*name));
            if let Some(i) = n.params().iter().position(|p| !p.is_empty() && p == key) {
                let i = u32::try_from(i).unwrap_or(0);
                if i < npos {
                    let q = qualname(n);
                    return Err(self.err(
                        T::TypeError,
                        &format!(
                            "argument for {q}() given by name ('{key}') and position ({})",
                            i + 1
                        ),
                    ));
                }
                self.stack[(top + i) as usize] = val;
                npos_out = npos_out.max(i + 1);
            } else if let Some(j) = n.kwonly().iter().position(|p| p == key) {
                self.stack[(top + max_pos + u32::try_from(j).unwrap_or(0)) as usize] = val;
            } else if n.kwonly().contains("**") {
                let d = if let Some(d) = extra {
                    d
                } else {
                    let d = self.new_dict()?;
                    extra = Some(d);
                    d
                };
                self.dict_set(d, *name, val)?;
            } else {
                let q = qualname(n);
                // CPython's argument-clinic builtins say "unexpected"; the
                // few still parsed by `PyArg_ParseTupleAndKeywords` do not.
                let msg = if n.name() == "enumerate" {
                    format!("'{key}' is an invalid keyword argument for {q}()")
                } else if q == "dir" {
                    // Its one keyword is the compiler's hidden `<scope>`.
                    String::from("dir() takes no keyword arguments")
                } else {
                    // `sorted` reports through the `list.sort` it calls;
                    // the clinic names methods bare.
                    let q = if n.name() == "sorted" {
                        "sort"
                    } else if n.owner.is_some() {
                        n.name()
                    } else {
                        q.as_str()
                    };
                    format!("{q}() got an unexpected keyword argument '{key}'")
                };
                return Err(self.err(T::TypeError, &msg));
            }
        }
        if let Some(d) = extra
            && let Some(j) = n.kwonly().iter().position(|p| p == "**")
        {
            self.stack[(top + max_pos + u32::try_from(j).unwrap_or(0)) as usize] = d;
        }
        // Positional parameters given by keyword must be contiguous.
        for i in 0..npos_out {
            if self.stack[(top + i) as usize] == Value::UNDEFINED && i < u32::from(n.min) {
                let q = clinic_name(n);
                let pname = n.params().get(i as usize).unwrap_or("");
                // A class method's position does not count its class.
                let pos = clinic_pos(n, i - u32::from(is_classmethod(n)));
                return Err(self.err(
                    T::TypeError,
                    &format!("{q}() missing required argument '{pname}'{pos}"),
                ));
            }
        }
        self.check_arity(n, npos_out.max(u32::from(n.min).min(npos_out)))?;
        let args = CallArgs {
            base: top,
            n: npos_out,
            kw: top + max_pos,
        };
        if !self.consumed.is_empty() {
            self.consumed.clear();
        }
        if direct && self.fuel.is_some() && self.native_short(n, &args) {
            return Err(Raised);
        }
        let r = (n.f)(self, args);
        if r.is_err() && self.slow {
            self.slow = false;
            return self.hand_to_twin(idx, n, args);
        }
        // Out of memory that garbage may hold: called straight from `CALL`,
        // it may run again after a collection (`Vm::oom_retry`).
        if r.is_err() && direct && self.oom_retry(n) {
            return Err(Raised);
        }
        r
    }

    /// A builtin met user objects or generators it cannot handle without
    /// running Python code: it continues in its prelude twin
    /// (`_name` or `_type_name`), called with the positional arguments as
    /// a tuple and then each keyword-only value (`_MISSING` if absent).
    /// Builtins without a twin run again on materialized iterables
    /// (`_mcall`).
    fn hand_to_twin(&mut self, idx: u32, n: &Native, a: CallArgs) -> Result<Value, Raised> {
        let name = match n.owner {
            Some(o) => format!("_{}_{}", o.name(), n.name()),
            None => format!("_{}", n.name()),
        };
        let mut pos = self.args_vec(&a);
        if self.partial.is_some() {
            // `collect` stopped part way through an argument: the native
            // itself runs again once the prelude has gathered the rest.
            self.consumed.clear();
            let kw = self.native_kwargs(n, &a)?;
            let (f, args, _) = self.slow_call(Value::native(idx), pos.clone(), kw)?;
            if f != Value::native(idx) {
                return Ok(self.request_call(f, args, Cont::None));
            }
        }
        for p in &mut pos {
            *p = self.unconsume(*p)?;
        }
        self.consumed.clear();
        let t = self.alloc_tuple(&pos)?;
        let mut args = alloc::vec![t];
        for j in 0..u32::try_from(n.kwonly().len()).unwrap_or(0) {
            let v = self.kwarg(&a, j);
            args.push(if v == Value::UNDEFINED {
                self.missing
            } else {
                v
            });
        }
        let k = self.intern(&name)?;
        // A native of a bundled Python module continues in that module's
        // `_<name>_py`, a method of a module's type in its `_<Type>_<name>`.
        let module_twin = match (n.owner, n.module()) {
            (None, Some(m)) => self.std_global(m, &format!("_{}_py", n.name()))?,
            (Some(o), _) => self.std_global(o.module(), &name)?,
            _ => None,
        };
        // A module's native never continues in the twin of the builtin of
        // the same name (`math.pow` is not `pow`).
        let shadows = n.module().is_some()
            && NATIVES
                .iter()
                .any(|b| b.owner.is_none() && b.module == 0 && b.name() == n.name());
        let twin = match module_twin {
            Some(f) => Some(f),
            None if shadows => None,
            None => self.prelude_get(k)?,
        };
        if let Some(f) = twin {
            self.request = Some(Request::Call {
                func: f,
                args: args.into(),
                kw: Value::NONE,
                cont: Cont::None,
                outer: Cont::None,
            });
            return Ok(Value::UNDEFINED);
        }
        if n.owner.is_some() && n.name().starts_with("__") && n.name().ends_with("__") {
            // A slot wrapper: one generic twin.
            let name = self.new_str(n.name())?;
            return self.prelude_request("_slot", alloc::vec![name, t], Cont::None);
        }
        // Keyword-only values back into a dict for the generic fallback.
        let kw = self.native_kwargs(n, &a)?;
        let f = Value::native(idx);
        self.prelude_request("_mcall", alloc::vec![f, t, kw], Cont::None)
    }

    /// Global `name` of bundled Python module `module`, once it is loaded.
    pub(crate) fn std_global(&mut self, module: &str, name: &str) -> Result<Option<Value>, Raised> {
        let g = self
            .stdlib
            .iter()
            .position(|s| s.name == module)
            .and_then(|i| self.std_globals.get(i).copied())
            .filter(|g| *g != Value::NONE);
        match g {
            Some(g) => {
                let k = self.intern(name)?;
                Ok(self.dict_get_str(g, k))
            }
            None => Ok(None),
        }
    }

    /// A native call's keyword-only values as a dict (absent ones left out).
    fn native_kwargs(&mut self, n: &Native, a: &CallArgs) -> Result<Value, Raised> {
        let kw = self.new_dict()?;
        for (j, kname) in n.kwonly().iter().enumerate() {
            let v = self.kwarg(a, u32::try_from(j).unwrap_or(0));
            if v != Value::UNDEFINED {
                if kname == "**" {
                    for (k2, v2) in self.dict_items_vec(v) {
                        self.dict_set(kw, k2, v2)?;
                    }
                } else {
                    let kk = self.intern(kname)?;
                    self.dict_set(kw, kk, v)?;
                }
            }
        }
        Ok(kw)
    }

    fn check_arity(&mut self, n: &Native, got: u32) -> Result<(), Raised> {
        let (min, max) = (u32::from(n.min), u32::from(n.max));
        if got >= min && (n.max == VAR || got <= max) {
            return Ok(());
        }
        let method = n.owner.is_some();
        if method && got == 0 {
            // An unbound method called with no receiver.
            let owner = n.owner.map_or("object", T::name);
            let msg = format!(
                "descriptor '{}' of '{owner}' object needs an argument",
                n.name()
            );
            return Err(self.err(T::TypeError, &msg));
        }
        let (got, min, max) = if method {
            (got.saturating_sub(1), min - 1, max.saturating_sub(1))
        } else {
            (got, min, max)
        };
        let q = qualname(n);
        let named = n
            .params()
            .get(got as usize + usize::from(method))
            .unwrap_or("");
        let msg = if n.module() == Some("asyncio") {
            // asyncio's functions are Python `def`s in CPython, each with
            // at most one required parameter.
            let s = if max == 1 { "" } else { "s" };
            if got < min {
                format!("{q}() missing 1 required positional argument: '{named}'")
            } else if min == max {
                format!("{q}() takes {max} positional argument{s} but {got} were given")
            } else {
                format!("{q}() takes from {min} to {max} positional arguments but {got} were given")
            }
        } else if got < min && !named.is_empty() {
            let pos = clinic_pos(n, got);
            format!(
                "{}() missing required argument '{named}'{pos}",
                clinic_name(n)
            )
        } else if min == max && !method && n.module().is_some() && !n.kwonly().is_empty() {
            // Clinic functions with positional-only and keyword-only
            // parameters count the positional ones.
            let s = if min == 1 { "" } else { "s" };
            format!("{q}() takes exactly {min} positional argument{s} ({got} given)")
        } else if min == 1 && max == 1 && n.name() != "sorted" {
            // METH_O functions of modules are named with their module.
            match (n.owner, n.module()) {
                (None, Some(m)) if m != "_mm" => {
                    format!("{m}.{q}() takes exactly one argument ({got} given)")
                }
                _ => format!("{q}() takes exactly one argument ({got} given)"),
            }
        } else if min == 0 && max == 0 {
            format!("{q}() takes no arguments ({got} given)")
        } else if method && got > max && n.params().iter().any(|p| !p.is_empty()) {
            // A clinic method taking keywords; with keyword-only ones too,
            // it counts the positional ones.
            let s = if max == 1 { "" } else { "s" };
            let what = if n.kwonly().is_empty() {
                ""
            } else {
                "positional "
            };
            format!(
                "{}() takes at most {max} {what}argument{s} ({got} given)",
                n.name()
            )
        } else {
            let s = |k: u32| if k == 1 { "" } else { "s" };
            if min == max {
                format!("{} expected {min} argument{}, got {got}", n.name(), s(min))
            } else if got < min {
                format!(
                    "{} expected at least {min} argument{}, got {got}",
                    n.name(),
                    s(min)
                )
            } else if matches!(
                n.name(),
                "complex" | "bytes" | "pow" | "round" | "sum" | "__import__"
            ) {
                format!("{}() takes at most {max} arguments ({got} given)", n.name())
            } else {
                format!(
                    "{} expected at most {max} argument{}, got {got}",
                    n.name(),
                    s(max)
                )
            }
        };
        Err(self.err(T::TypeError, &msg))
    }

    /// `self` of a method `set` and `frozenset` share.
    fn check_anyset(&mut self, a: &CallArgs, name: &str) -> Result<Value, Raised> {
        let s = self.arg_o(a, 0);
        if self.type_of(s) == T::frozenset {
            return Ok(s);
        }
        self.check_self(a, T::set, name)
    }

    fn check_self(&mut self, a: &CallArgs, owner: T, name: &str) -> Result<Value, Raised> {
        let s = self.arg(a, 0);
        let t = self.type_of(s);
        if t == owner || (owner == T::int && t == T::bool) {
            return Ok(s);
        }
        Err(self.err(
            T::TypeError,
            &format!(
                "descriptor '{name}' for '{}' objects doesn't apply to a '{}' object",
                owner.name(),
                t.name()
            ),
        ))
    }
}

/// A module function whose body is its module's Python `_<name>_py`, or a
/// builtin whose body is its prelude twin.
fn to_module_twin(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    Err(vm.need_py())
}

/// The argument clinic's " (pos N)" after a missing argument `i` (0-based,
/// after any receiver) of a builtin taking keywords.
fn clinic_pos(n: &Native, i: u32) -> String {
    if n.name() == "enumerate" {
        String::new()
    } else {
        format!(" (pos {})", i + 1)
    }
}

/// The name a missing-argument message gives: class methods, which the
/// clinic names bare, as `from_bytes()`, others by [`qualname`].
fn clinic_name(n: &Native) -> String {
    if is_classmethod(n) {
        n.name().into()
    } else {
        qualname(n)
    }
}

fn qualname(n: &Native) -> String {
    match n.owner {
        Some(o) => format!("{}.{}", o.name(), n.name()),
        None => n.name().into(),
    }
}

// ------------------------------------------------------------ functions

fn bi_print(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let sep = vm.kwarg(&a, 0);
    let end = vm.kwarg(&a, 1);
    for (v, what) in [(sep, "sep"), (end, "end")] {
        if v == Value::UNDEFINED || v == Value::NONE {
            continue;
        }
        if !Vm::is_str(v) {
            return Err(vm.err(
                T::TypeError,
                &format!("{what} must be None or a string, not {}", vm.type_name(v)),
            ));
        }
    }
    let file = vm.kwarg(&a, 2);
    let stream = if file == Value::UNDEFINED || file == Value::NONE {
        if vm.replaced_stream("stdout")?.is_some() {
            // The prelude twin writes to the replacement.
            return Err(vm.need_py());
        }
        Stream::Stdout
    } else if let Some(s) = vm.stream_of(file) {
        s
    } else {
        // Any object with a `write` method: the prelude twin calls it.
        return Err(vm.need_py());
    };
    let mut s = NativeText::default();
    let begun = vm.work;
    for i in 0..a.n {
        if i > 0 {
            if Vm::is_str(sep) {
                s.push_value(vm, sep)?;
            } else {
                s.push(vm, " ")?;
            }
        }
        let v = vm.arg_o(&a, i);
        let text = if Vm::is_str(v) {
            Some(v)
        } else {
            vm.str_backing(v)
        };
        if let Some(text) = text {
            s.push_value(vm, text)?;
        } else {
            // Past a step's work (big ints' conversions): the twin, which
            // converts each argument in a call of its own.
            if i > 0 && vm.work.wrapping_sub(begun) + vm.decimal_units(v) > nstep::STEP_WORK {
                return Err(vm.need_py());
            }
            // CPython has written the earlier arguments when `str()` raises
            // (`print("a", 10 ** 5000)` leaves `a `); a hand-off to the
            // prelude twin writes nothing here, since the twin starts over.
            let text = match vm.stringify(v) {
                Ok(text) => text,
                Err(e) => {
                    if !vm.slow && !s.text.is_empty() {
                        vm.write_out(stream, &s.text);
                    }
                    return Err(e);
                }
            };
            s.push(vm, &text)?;
        }
    }
    if Vm::is_str(end) {
        s.push_value(vm, end)?;
    } else {
        s.push(vm, "\n")?;
    }
    vm.write_out(stream, &s.text);
    Ok(Value::NONE)
}

fn stream_write(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let me = vm.check_self(&a, T::TextIOWrapper, "write")?;
    let s = vm.arg_o(&a, 1);
    if !Vm::is_str(s) {
        return Err(vm.err(
            T::TypeError,
            &format!("write() argument must be str, not {}", vm.type_name(s)),
        ));
    }
    let stream = vm.stream_of(me).unwrap_or(Stream::Stdout);
    if let Some(out) = vm.output.as_mut() {
        let text = s
            .as_obj()
            .and_then(|r| vm.heap.as_str(r).ok())
            .unwrap_or("");
        out(stream, text);
    }
    vm.new_int(i64::try_from(vm.str_len(s)).unwrap_or(i64::MAX))
}

fn stream_flush(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.check_self(&a, T::TextIOWrapper, "flush")?;
    Ok(Value::NONE)
}

#[cfg(feature = "asyncio")]
fn aio_gather(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let items = vm.args_vec(&a);
    let keep = vm.kwarg(&a, 0);
    let keep = keep != Value::UNDEFINED && vm.truthy(keep);
    vm.gather(&items, keep)
}

/// `_aio_run(main, root)`: `asyncio.run(main)` runs `root`, the prelude's
/// coroutine that awaits `main` and then shuts the loop down.
#[cfg(feature = "asyncio")]
fn aio_run(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (c, root) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !c.is_obj_kind(ty::CORO as u8) || !root.is_obj_kind(ty::CORO as u8) {
        return Err(vm.err(
            T::TypeError,
            "An asyncio.Future, a coroutine or an awaitable is required",
        ));
    }
    // `_mm.aio_run(main, root, 1)`: a `run_until_complete()`; 2: of a
    // future made before it.
    let open = vm.arg_o(&a, 2);
    if open != Value::UNDEFINED && open != Value::FALSE {
        vm.mark_run_open(root, open != Value::TRUE);
    }
    // The dispatch loop runs the coroutine in place (no nested loop).
    vm.request = Some(Request::Run(root));
    Ok(Value::UNDEFINED)
}

#[cfg(feature = "asyncio")]
fn aio_create_task(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let c = vm.arg_o(&a, 0);
    // `asyncio.sleep()` is a coroutine on CPython.
    if !c.is_obj_kind(ty::CORO as u8) && !c.is_obj_kind(ty::SLEEP as u8) {
        let r = vm.repr_whole(c)?;
        return Err(vm.err(T::TypeError, &format!("a coroutine was expected, got {r}")));
    }
    let name = vm.kwarg(&a, 0);
    let context = vm.kwarg(&a, 1);
    if context != Value::UNDEFINED && context != Value::NONE {
        let name = if name == Value::UNDEFINED {
            Value::NONE
        } else {
            name
        };
        return vm.prelude_request("_aio_create_ctx", alloc::vec![c, name, context], Cont::None);
    }
    match name {
        Value::UNDEFINED | Value::NONE => vm.spawn(c),
        n if Vm::is_str(n) => vm.spawn_named(c, n),
        // `str(name)` may run Python code.
        n => vm.prelude_request("_aio_create_named", alloc::vec![c, n], Cont::None),
    }
}

/// `_mm.ctx_get()`: the running `contextvars` context, a dict.
fn mm_ctx_get(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    vm.context_dict()
}

/// `_mm.ctx_swap(d)`: makes dict `d` (or `None`, empty) the running
/// context; returns the one it replaces.
fn mm_ctx_swap(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let d = vm.arg_o(&a, 0);
    if d != Value::NONE && !d.is_obj_kind(ty::DICT as u8) {
        return Err(vm.err(T::TypeError, "a context dict is required"));
    }
    Ok(core::mem::replace(&mut vm.context, d))
}

/// `_mm.ctx_copy()`: a copy of the running context, `None` when empty.
fn mm_ctx_copy(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    vm.context_copy()
}

/// `_mm.aio_unretrieved()`: the failed tasks and futures whose exception
/// nobody retrieved, which stop being tracked.
#[cfg(feature = "asyncio")]
fn aio_unretrieved(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    let all = vm.take_unretrieved();
    vm.new_list(&all)
}

/// `_mm.aio_escape()`: raises the `KeyboardInterrupt` or `SystemExit` a
/// task raised during this `asyncio.run`, as the loop let it out (no
/// context: CPython's leaves the loop, not a handler).
#[cfg(feature = "asyncio")]
fn aio_escape(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    let e = core::mem::replace(&mut vm.escape, Value::NONE);
    if e == Value::NONE {
        return Ok(Value::NONE);
    }
    vm.handled = Value::NONE;
    vm.exc = e;
    Err(Raised)
}

/// `_mm.aio_task_ctx(task, d)`: the task (not started) runs in dict `d`.
#[cfg(feature = "asyncio")]
fn aio_task_ctx(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (t, d) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !t.is_obj_kind(ty::TASK as u8) || !d.is_obj_kind(ty::DICT as u8) {
        return Err(vm.err(T::TypeError, "a task and a context dict are required"));
    }
    vm.set_task_context(t, d);
    Ok(Value::NONE)
}

#[cfg(feature = "asyncio")]
fn aio_spawn_internal(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let c = vm.arg_o(&a, 0);
    if !c.is_obj_kind(ty::CORO as u8) {
        return Err(vm.err(T::TypeError, "a coroutine is required"));
    }
    vm.spawn_named(c, Value::FALSE)
}

#[cfg(feature = "asyncio")]
fn aio_get_name(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.task_name(vm.arg_o(&a, 0))
}

#[cfg(feature = "asyncio")]
fn aio_set_name(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (t, v) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if Vm::is_str(v) {
        vm.set_task_name(t, v)?;
        return Ok(Value::NONE);
    }
    vm.prelude_request("_aio_set_name", alloc::vec![t, v], Cont::None)
}

#[cfg(feature = "asyncio")]
fn aio_name_set(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (t, v) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !t.is_obj_kind(ty::TASK as u8) || !Vm::is_str(v) {
        return Err(vm.err(T::TypeError, "a task and a str are required"));
    }
    vm.set_task_name(t, v)?;
    Ok(Value::NONE)
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
#[cfg(feature = "asyncio")]
fn aio_get_coro(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let c = vm.aw_extra(vm.arg_o(&a, 0));
    Ok(if c.is_obj_kind(ty::CORO as u8) {
        c
    } else {
        Value::NONE
    })
}

#[cfg(feature = "asyncio")]
fn aio_get_loop(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    vm.prelude_request("_aio_the_loop", alloc::vec![], Cont::None)
}

#[cfg(feature = "asyncio")]
fn aio_add_cb(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (f, cb) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    vm.add_done_callback(f, cb)?;
    Ok(Value::NONE)
}

#[cfg(feature = "asyncio")]
fn aio_remove_cb(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (f, cb) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    let n = vm.remove_done_callback(f, cb)?;
    vm.new_int(i64::try_from(n).unwrap_or(i64::MAX))
}

/// `future.set_result(result)` of a loop future.
#[cfg(feature = "asyncio")]
fn aio_set_result(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (f, v) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !vm.is_loop_future(f) {
        return Err(vm.err(
            T::RuntimeError,
            "micro-monty: only asyncio.Future() futures can be settled by the program",
        ));
    }
    if !vm.settle_pending(f, true, v) {
        return Err(vm.err(T::InvalidStateError, "invalid state"));
    }
    Ok(Value::NONE)
}

#[cfg(feature = "asyncio")]
fn aio_set_exception(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (f, e) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    vm.prelude_request("_aio_set_exception", alloc::vec![f, e], Cont::None)
}

/// `_aio_set_exc(future, exc)` with `exc` an exception instance.
#[cfg(feature = "asyncio")]
fn aio_set_exc(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (f, e) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !vm.is_loop_future(f) {
        return Err(vm.err(
            T::RuntimeError,
            "micro-monty: only asyncio.Future() futures can be settled by the program",
        ));
    }
    if !e.is_obj_kind(ty::EXC as u8) && !vm.instance_of(e, vm.type_obj(T::BaseException)) {
        return Err(vm.err(T::TypeError, "invalid exception object"));
    }
    if !vm.settle_pending(f, false, e) {
        return Err(vm.err(T::InvalidStateError, "invalid state"));
    }
    vm.note_unretrieved(f);
    Ok(Value::NONE)
}

#[cfg(feature = "asyncio")]
fn aio_soon(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.call_soon(vm.arg_o(&a, 0))?;
    Ok(Value::NONE)
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
#[cfg(feature = "asyncio")]
fn aio_time(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    Ok(Value::float(vm.loop_time()))
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
#[cfg(feature = "asyncio")]
fn aio_running(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    Ok(Value::from_bool(vm.loop_running()))
}

#[cfg(feature = "asyncio")]
fn aio_all_tasks(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    let tasks = vm.all_tasks(0);
    vm.new_list(&tasks)
}

/// `Task`/`Future` methods: `done()`.
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn aio_done(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    Ok(Value::from_bool(vm.aw_state(vm.arg_o(&a, 0)) != 0))
}

/// Is awaitable `f` cancelled?
fn aio_is_cancelled(vm: &Vm, f: Value) -> bool {
    vm.aw_state(f) == crate::aio::aw::CANCELLED
}

fn aio_result(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = vm.arg_o(&a, 0);
    match vm.aw_state(f) {
        0 => Err(vm.err(T::InvalidStateError, "Result is not set.")),
        1 => Ok(vm.aw_result(f)),
        _ if aio_is_cancelled(vm, f) => {
            vm.exc = vm.cancelled_error(f)?;
            Err(Raised)
        }
        _ => {
            vm.retrieved(f);
            vm.exc = vm.aw_result(f);
            Err(Raised)
        }
    }
}

fn aio_exception(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = vm.arg_o(&a, 0);
    match vm.aw_state(f) {
        0 => Err(vm.err(T::InvalidStateError, "Exception is not set.")),
        1 => Ok(Value::NONE),
        _ if aio_is_cancelled(vm, f) => {
            vm.exc = vm.cancelled_error(f)?;
            Err(Raised)
        }
        _ => {
            vm.retrieved(f);
            Ok(vm.aw_result(f))
        }
    }
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn aio_cancelled(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    Ok(Value::from_bool(aio_is_cancelled(vm, vm.arg_o(&a, 0))))
}

/// `cancel(msg=None)`, as CPython's (docs/spec.md §8.5): a pending future
/// settles cancelled; a task passes the cancel to what it awaits, or
/// raises `CancelledError` at its next step; a gather cancels its children.
fn aio_cancel(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = vm.arg_o(&a, 0);
    let msg = match vm.arg_o(&a, 1) {
        Value::UNDEFINED => Value::NONE,
        m => m,
    };
    vm.cancel_tree(f, msg).map(Value::from_bool)
}

#[cfg(feature = "asyncio")]
fn aio_cancelling(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.cancelling(vm.arg_o(&a, 0));
    vm.new_int(i64::try_from(n).unwrap_or(i64::MAX))
}

#[cfg(feature = "asyncio")]
fn aio_uncancel(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.uncancel(vm.arg_o(&a, 0));
    vm.new_int(i64::try_from(n).unwrap_or(i64::MAX))
}

#[cfg(feature = "asyncio")]
fn aio_first(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let items = vm.arg_o(&a, 0);
    let items = vm.list_items(items);
    vm.first_of(&items)
}

#[cfg(feature = "asyncio")]
fn aio_deadline(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.deadline(vm.arg_o(&a, 0))
}

#[cfg(feature = "asyncio")]
pub(crate) fn aio_future(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    vm.loop_future()
}

/// `_aio_set(future, value)`: `future.set_result(value)`.
#[cfg(feature = "asyncio")]
pub(crate) fn aio_set(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (f, v) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !vm.is_loop_future(f) {
        return Err(vm.err(T::TypeError, "an asyncio.Future is required"));
    }
    if !vm.settle_pending(f, true, v) {
        return Err(vm.err(T::InvalidStateError, "invalid state"));
    }
    Ok(Value::NONE)
}

#[cfg(feature = "asyncio")]
fn aio_current_task(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    vm.current_task()
}

#[cfg(feature = "asyncio")]
fn aio_tasks(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    let tasks = vm.other_tasks();
    vm.new_list(&tasks)
}

#[cfg(feature = "asyncio")]
fn aio_stop(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.int_arg(vm.arg_o(&a, 0), "")?;
    vm.stop_future(u32::try_from(n).unwrap_or(1))
}

/// `_aio_main_end(ok, value)`: `asyncio.run`'s coroutine returned
/// `value` (`ok`) or raised it.
#[cfg(feature = "asyncio")]
fn aio_main_end(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let ok = vm.arg_o(&a, 0) == Value::TRUE;
    vm.main_ended(ok, vm.arg_o(&a, 1))
}

#[cfg(feature = "asyncio")]
fn aio_sleep(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (d, r) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    // A host function registered as `asyncio.sleep` replaces the virtual
    // sleep (it answers with a future, or raises to refuse sleeping).
    if let Some(f) = vm
        .find_interned("asyncio.sleep")
        .and_then(|k| vm.dict_get_str(vm.builtins, k))
    {
        let mut args = alloc::vec![d];
        if r != Value::UNDEFINED {
            args.push(r);
        }
        return vm.call_request(f, args, Cont::None);
    }
    vm.sleep(d, r)
}

fn bi_len(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if let Some(r) = vm.dunder_call(v, Dn::Len, &[], Cont::Len)? {
        return Ok(r);
    }
    let n = vm.len_of(v)?;
    vm.new_int(i64::try_from(n).unwrap_or(i64::MAX))
}

fn bi_repr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if let Some(r) = vm.dunder_call(v, Dn::Repr, &[], Cont::Check(check::REPR))? {
        return Ok(r);
    }
    if let Some(r) = crate::strmeth::repr_big(vm, v, false) {
        return r;
    }
    let s = vm.repr(v)?;
    vm.new_str(&s)
}

fn bi_ascii(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if vm.uflags(v) & crate::class::f::REPR != 0 {
        return Err(vm.need_py());
    }
    if let Some(r) = crate::strmeth::repr_big(vm, v, true) {
        return r;
    }
    let s = vm.repr(v)?;
    let ascii = crate::percent::ascii_escape(vm, s)?;
    vm.new_str(&ascii)
}

/// Whether `hit` holds for a class in `cls`: a class or tuples of them,
/// nested to any depth, tried left to right as CPython does. The walk is
/// iterative and visits at most `dict::WALK` items, charged as fuel;
/// `None` means the prelude must finish it in bytecode: the walk ran out,
/// or an item defines `check` (`__instancecheck__`/`__subclasscheck__`).
fn any_class(
    vm: &mut Vm,
    cls: Value,
    check: Dn,
    msg: &str,
    hit: &dyn Fn(&Vm, Value) -> bool,
) -> Result<Option<bool>, Raised> {
    let mut left = crate::dict::WALK;
    let mut todo = alloc::vec![cls];
    let r = loop {
        let Some(c) = todo.pop() else {
            break Ok(Some(false));
        };
        if left == 0 || vm.has_check(c, check) {
            break Ok(None);
        }
        left -= 1;
        if Vm::is_type(c) {
            if hit(vm, c) {
                break Ok(Some(true));
            }
        } else if c.is_obj_kind(ty::TUPLE as u8) {
            todo.extend(vm.tuple_items(c).into_iter().rev());
        } else {
            break Err(vm.err(T::TypeError, msg));
        }
    };
    vm.walked(left);
    r
}

fn bi_isinstance(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (v, c) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if let Some(r) = vm.meta_check(v, c, Dn::Instancecheck)? {
        return Ok(r);
    }
    if Vm::is_type(c) {
        return Ok(Value::from_bool(vm.instance_of(v, c)));
    }
    match any_class(
        vm,
        c,
        Dn::Instancecheck,
        "isinstance() arg 2 must be a type, a tuple of types, or a union",
        &|vm, c| vm.instance_of(v, c),
    )? {
        Some(r) => Ok(Value::from_bool(r)),
        None => vm.prelude_request("_isinstance_any", alloc::vec![v, c], Cont::None),
    }
}

impl Vm {
    /// Is `c` a class with a metaclass, or any instance (a `typing.Union`),
    /// whose type defines `name` (`__instancecheck__`/`__subclasscheck__`)?
    fn has_check(&mut self, c: Value, name: Dn) -> bool {
        (self.meta_of(c).is_some() || (!Vm::is_class(c) && self.user_class(c).is_some()))
            && self.user_method(c, name).is_some()
    }

    /// `isinstance`/`issubclass` against a class whose metaclass defines
    /// `name` (`__instancecheck__`/`__subclasscheck__`): its call, or the
    /// prelude's for a tuple holding such a class.
    fn meta_check(&mut self, v: Value, c: Value, name: Dn) -> Result<Option<Value>, Raised> {
        if c.is_obj_kind(ty::TUPLE as u8) {
            for x in self.tuple_items(c) {
                if self.has_check(x, name) {
                    let f = if name == Dn::Instancecheck {
                        "_isinstance_any"
                    } else {
                        "_issubclass_any"
                    };
                    return self
                        .prelude_request(f, alloc::vec![v, c], Cont::None)
                        .map(Some);
                }
            }
            return Ok(None);
        }
        if !self.has_check(c, name) || (name == Dn::Instancecheck && self.class_of(v) == c) {
            return Ok(None);
        }
        self.dunder_call(c, name, &[v], Cont::None)
    }
}

/// `type.__new__(mcls, name, bases, ns, **kwargs)`.
fn type_dunder_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let kw = match vm.kwarg(&a, 0) {
        Value::UNDEFINED => Value::NONE,
        kw => kw,
    };
    let (m, name, bases, ns) = (
        vm.arg_o(&a, 0),
        vm.arg_o(&a, 1),
        vm.arg_o(&a, 2),
        vm.arg_o(&a, 3),
    );
    if !Vm::is_type(m) || !vm.subclass_of(m, vm.type_obj(T::type_)) {
        let msg = format!(
            "type.__new__(X): X is not a type object ({})",
            vm.type_name(m)
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    vm.type_new(name, bases, ns, kw, m)
}

/// `type.__init__(cls, ...)`: nothing to do.
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn type_dunder_init(_vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    Ok(Value::NONE)
}

/// `type.__call__(cls, *args, **kwargs)`: a metaclass's
/// `super().__call__()`, in the prelude.
fn type_dunder_call(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let args = vm.args_vec(&a);
    let rest = vm.alloc_tuple(args.get(1..).unwrap_or(&[]))?;
    let kw = match vm.kwarg(&a, 0) {
        Value::UNDEFINED => vm.new_dict()?,
        kw => kw,
    };
    vm.prelude_request(
        "_type_call",
        alloc::vec![vm.arg_o(&a, 0), rest, kw],
        Cont::None,
    )
}

/// `_build_class(name, bases, ns, kwargs, cell)`: a class statement's class
/// from bases `_build_bases` resolved.
fn pl_build_class(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let [name, bases, ns, kw, cell] = [0, 1, 2, 3, 4].map(|i| vm.arg_o(&a, i));
    vm.build_resolved(name, bases, ns, kw, cell, Value::NONE)
}

/// `_cell_set(cell, v)`: a metaclass-built class fills `__class__`.
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn pl_cell_set(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let cell = vm.arg_o(&a, 0);
    if cell.is_obj_kind(ty::CELL as u8)
        && let Some(r) = cell.as_obj()
    {
        vm.set_slot_o(r, 0, vm.arg_o(&a, 1));
    }
    Ok(Value::NONE)
}

fn bi_abs(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if let Some(r) = vm.dunder_call(v, Dn::Abs, &[], Cont::None)? {
        return Ok(r);
    }
    // A subclass of a numeric type without its own `__abs__`: its value's.
    let n = vm.unbx(v);
    if let Some(i) = vm.int_of_o(n) {
        return vm.int_from_i128(i128::from(i).abs());
    }
    if let Some(b) = vm.big_of(n) {
        return vm.new_big(&b.abs());
    }
    if let Some(f) = n.as_float() {
        return Ok(Value::float(f.abs()));
    }
    if let Some((re, im)) = vm.complex_of(n) {
        return vm.complex_abs(re, im);
    }
    let msg = format!("bad operand type for abs(): '{}'", vm.type_name(v));
    Err(vm.err(T::TypeError, &msg))
}

fn min_max(vm: &mut Vm, a: CallArgs, want: Ordering, name: &str) -> Result<Value, Raised> {
    let key = vm.kwarg(&a, 0);
    if key != Value::UNDEFINED && key != Value::NONE {
        return Err(vm.need_py());
    }
    if a.n > 1 && vm.kwarg(&a, 1) != Value::UNDEFINED {
        let msg =
            format!("Cannot specify a default for {name}() with multiple positional arguments");
        return Err(vm.err(T::TypeError, &msg));
    }
    if a.n == 1 {
        let it = vm.get_iter(vm.arg_o(&a, 0))?;
        let kind = if want == Ordering::Less {
            fold::MIN
        } else {
            fold::MAX
        };
        return fold_start(vm, kind, it, Value::UNDEFINED, vm.kwarg(&a, 1));
    }
    let start = a.base as usize;
    let args = vm.stack.get(start..start + a.n as usize).unwrap_or(&[]);
    let res = NativeValues::copy(&mut vm.heap, args);
    // One unit per item compared, as a fold charges an iterable.
    vm.burn(u64::from(a.n));
    let items = vm.hx_o(res)?;
    let Some(mut best) = items.first().copied() else {
        let d = vm.kwarg(&a, 1);
        if d != Value::UNDEFINED {
            return Ok(d);
        }
        return Err(vm.err(
            T::ValueError,
            &format!("{name}() iterable argument is empty"),
        ));
    };
    let sym = if want == Ordering::Less { "<" } else { ">" };
    for &x in &items[1..] {
        // CPython compares item < best for min and item > best for max.
        if vm.order(x, best, sym)? == Some(want) {
            best = x;
        }
    }
    Ok(best)
}

fn bi_min(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    min_max(vm, a, Ordering::Less, "min")
}

fn bi_max(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    min_max(vm, a, Ordering::Greater, "max")
}

/// A fold's kind: raw word 1 of an `nstep::op::FOLD` state.
mod fold {
    pub const ANY: u64 = 0;
    pub const ALL: u64 = 1;
    pub const MIN: u64 = 2;
    pub const MAX: u64 = 3;
    pub const SUM: u64 = 4;
}

/// `sum`'s stage, raw word 2 of a fold: CPython's `builtin_sum_impl` runs
/// these blocks in order, each while its total keeps its type and the
/// item is one it takes, then adds that item with `+` and goes on in the
/// first later block whose type the total has, never back. Their totals
/// sit in raw words 3 to 6: an `i64`; a float and its Neumaier
/// compensation; a complex number's two such pairs. `prelude.py`'s
/// `_sum_rest` runs the same blocks.
mod stage {
    pub const INT: u64 = 0;
    pub const FLOAT: u64 = 1;
    pub const COMPLEX: u64 = 2;
    pub const ANY: u64 = 3;
}

/// Fold `kind` over iterator `it` from `acc` (`UNDEFINED` for `min`/`max`
/// before an item); `extra` is `min`/`max`'s `default`. Each item is
/// taken as it is pulled, one fuel unit each, [`nstep::STEP_WORK`] units a
/// step; a longer fold goes on in `_steps` (`nstep::op::FOLD`), so an
/// endless range parks. An item, or a `next`, that needs Python code
/// hands the rest to the prelude with the result so far, so the iterator
/// is left where CPython leaves it when an item fails (docs/spec.md §5.1).
fn fold_start(
    vm: &mut Vm,
    kind: u64,
    it: Value,
    acc: Value,
    extra: Value,
) -> Result<Value, Raised> {
    let mut st = [acc, extra];
    let mut raw = [kind, stage::ANY, 0, 0, 0, 0];
    if kind == fold::SUM {
        raw[1] = sum_enter(vm, acc, stage::INT, &mut raw);
    }
    if let Some(v) = fold_run(vm, it, &mut st, &mut raw)? {
        return Ok(v);
    }
    vm.chunk_rest(nstep::op::FOLD, &[it, st[0], st[1]], &raw)
}

/// One step of a fold ([`fold_start`]).
pub(crate) fn fold_step(vm: &mut Vm, state: Value) -> Result<nstep::Step, Raised> {
    let Some(([it, acc, extra], [_, kind, stg, w0, w1, w2, w3])) =
        vm.nstate_get::<3, 7>(nstep::op::FOLD, state)
    else {
        return Err(vm.bad_step());
    };
    let mut st = [acc, extra];
    let mut raw = [kind, stg, w0, w1, w2, w3];
    if let Some(v) = fold_run(vm, it, &mut st, &mut raw)? {
        return Ok(nstep::Step::Done(v));
    }
    if let Some(r) = state.as_obj() {
        vm.set_slot_o(r, 1, st[0]);
    }
    vm.nstate_put(state, 1, &raw);
    Ok(nstep::Step::More)
}

/// The first `sum` stage from `from` on whose type total `acc` has, its
/// total set in `raw[2..]`.
fn sum_enter(vm: &Vm, acc: Value, from: u64, raw: &mut [u64; 6]) -> u64 {
    raw[2..].fill(0);
    let int = acc != Value::TRUE && acc != Value::FALSE;
    if from == stage::INT
        && int
        && let Some(i) = vm.int_of_o(acc)
    {
        raw[2] = i.cast_unsigned();
        stage::INT
    } else if from <= stage::FLOAT
        && let Some(f) = acc.as_float()
    {
        raw[2] = f.to_bits();
        stage::FLOAT
    } else if from <= stage::COMPLEX
        && let Some((re, im)) = vm.complex_of(acc)
    {
        raw[2] = re.to_bits();
        raw[4] = im.to_bits();
        stage::COMPLEX
    } else {
        stage::ANY
    }
}

/// Neumaier's step, CPython's `cs_add`: `x` added to the float total in
/// `w[0]` with its compensation in `w[1]`.
fn cs_add(w: &mut [u64], x: f64) {
    let (hi, mut lo) = (f64::from_bits(w[0]), f64::from_bits(w[1]));
    let t = hi + x;
    if hi.abs() >= x.abs() {
        lo += (hi - t) + x;
    } else {
        lo += (x - t) + hi;
    }
    w[0] = t.to_bits();
    w[1] = lo.to_bits();
}

/// A compensated total's value, CPython's `cs_to_double`.
fn cs_end(w: &[u64]) -> f64 {
    let (hi, lo) = (f64::from_bits(w[0]), f64::from_bits(w[1]));
    if lo != 0.0 && lo.is_finite() {
        hi + lo
    } else {
        hi
    }
}

/// A `sum` total as a value.
fn sum_value(vm: &mut Vm, acc: Value, raw: &[u64; 6]) -> Result<Value, Raised> {
    let w = &raw[2..];
    match raw[1] {
        stage::INT => vm.new_int(w[0].cast_signed()),
        stage::FLOAT => Ok(Value::float(cs_end(w))),
        stage::COMPLEX => vm.new_complex(cs_end(w), cs_end(&w[2..])),
        _ => Ok(acc),
    }
}

/// A `sum` total as `_sum_rest` takes it: the value and the
/// compensation, still apart.
fn sum_total(vm: &mut Vm, acc: Value, raw: &[u64; 6]) -> Result<(Value, Value), Raised> {
    let w = &raw[2..];
    Ok(match raw[1] {
        stage::INT => (vm.new_int(w[0].cast_signed())?, Value::float(0.0)),
        stage::FLOAT => (
            Value::float(f64::from_bits(w[0])),
            Value::float(f64::from_bits(w[1])),
        ),
        stage::COMPLEX => (
            vm.new_complex(f64::from_bits(w[0]), f64::from_bits(w[2]))?,
            vm.new_complex(f64::from_bits(w[1]), f64::from_bits(w[3]))?,
        ),
        _ => (acc, Value::float(0.0)),
    })
}

/// A step of a fold: `st` its result so far and `default`, `raw` its
/// kind and `sum`'s stage and totals ([`stage`]). The value, or `None`
/// with more to go.
fn fold_run(
    vm: &mut Vm,
    it: Value,
    st: &mut [Value; 2],
    raw: &mut [u64; 6],
) -> Result<Option<Value>, Raised> {
    let start = vm.work;
    let kind = raw[0];
    let mut n = 0u64;
    loop {
        if n + vm.work.wrapping_sub(start) >= nstep::STEP_WORK {
            vm.burn(n);
            return Ok(None);
        }
        let x = match vm.iter_next(it) {
            Ok(Some(x)) => x,
            Ok(None) => break,
            Err(e) => {
                vm.burn(n);
                if !vm.slow || matches!(kind, fold::MIN | fold::MAX) && st[0] == Value::UNDEFINED {
                    // Before an item `min`/`max` starts over in its twin.
                    return Err(e);
                }
                // `next` needs Python code: the rest goes on in the
                // prelude from the iterator, which it left unmoved.
                let (rest, args) = match kind {
                    fold::ANY => ("_any_rest", alloc::vec![Value::FALSE, it]),
                    fold::ALL => ("_all_rest", alloc::vec![Value::TRUE, it]),
                    fold::MIN | fold::MAX => {
                        let gt = Value::from_bool(kind == fold::MAX);
                        ("_minmax_rest", alloc::vec![st[0], vm.missing, it, gt])
                    }
                    _ => {
                        let (total, lo) = sum_total(vm, st[0], raw)?;
                        let stg = Value::int(raw[1].cast_signed()).unwrap_or(Value::NONE);
                        ("_sum_rest", alloc::vec![total, vm.missing, it, stg, lo])
                    }
                };
                vm.slow = false;
                let f = vm.prelude_fn(rest)?;
                return Ok(Some(vm.request_call(f, args, Cont::None)));
            }
        };
        n += 1;
        let (rest, args) = match kind {
            fold::ANY | fold::ALL => {
                let stop = kind == fold::ANY;
                match vm.truthy_fast(x) {
                    Some(t) if t == stop => {
                        vm.burn(n);
                        return Ok(Some(Value::from_bool(stop)));
                    }
                    Some(_) => continue,
                    None if stop => ("_any_rest", alloc::vec![x, it]),
                    None => ("_all_rest", alloc::vec![x, it]),
                }
            }
            fold::MIN | fold::MAX => {
                if st[0] == Value::UNDEFINED {
                    st[0] = x;
                    continue;
                }
                // CPython compares item < best for min and item > best for
                // max.
                let (want, sym) = if kind == fold::MIN {
                    (Ordering::Less, "<")
                } else {
                    (Ordering::Greater, ">")
                };
                match vm.order(x, st[0], sym) {
                    Ok(o) => {
                        if o == Some(want) {
                            st[0] = x;
                        }
                        continue;
                    }
                    Err(Raised) if vm.slow => {
                        let gt = Value::from_bool(kind == fold::MAX);
                        ("_minmax_rest", alloc::vec![st[0], x, it, gt])
                    }
                    Err(e) => {
                        vm.burn(n);
                        return Err(e);
                    }
                }
            }
            _ => {
                let stg = raw[1];
                let w = &mut raw[2..];
                let taken = match stg {
                    stage::INT => match vm.int_of_o(x) {
                        Some(b) if let Some(t) = w[0].cast_signed().checked_add(b) => {
                            w[0] = t.cast_unsigned();
                            true
                        }
                        _ => false,
                    },
                    stage::FLOAT | stage::COMPLEX => {
                        let c = if stg == stage::COMPLEX {
                            vm.complex_of(x)
                        } else {
                            None
                        };
                        let re = match c {
                            Some((re, _)) => Some(re),
                            None => match vm.num_to_f64(x) {
                                Ok(f) => f,
                                Err(e) => {
                                    vm.burn(n);
                                    return Err(e);
                                }
                            },
                        };
                        if let Some(re) = re {
                            cs_add(&mut w[..2], re);
                            if let Some((_, im)) = c {
                                cs_add(&mut w[2..], im);
                            }
                        }
                        re.is_some()
                    }
                    _ => false,
                };
                if taken {
                    continue;
                }
                // The stage ends at this item: its total as a value, `+`
                // the item, the next stage from the sum's type.
                let acc = sum_value(vm, st[0], raw)?;
                match vm.binop(mm_compile::ast::BinOp::Add, acc, x, false) {
                    Ok(v) => {
                        st[0] = v;
                        if stg != stage::ANY {
                            raw[1] = sum_enter(vm, v, stg + 1, raw);
                        }
                        continue;
                    }
                    Err(Raised) if vm.slow => {
                        let s = Value::int(stg.cast_signed()).unwrap_or(Value::NONE);
                        ("_sum_rest", alloc::vec![acc, x, it, s, Value::float(0.0)])
                    }
                    Err(e) => {
                        vm.burn(n);
                        return Err(e);
                    }
                }
            }
        };
        vm.slow = false;
        vm.burn(n);
        let f = vm.prelude_fn(rest)?;
        return Ok(Some(vm.request_call(f, args, Cont::None)));
    }
    vm.burn(n);
    Ok(Some(match kind {
        fold::ANY => Value::FALSE,
        fold::ALL => Value::TRUE,
        fold::MIN | fold::MAX if st[0] == Value::UNDEFINED => {
            if st[1] != Value::UNDEFINED {
                return Ok(Some(st[1]));
            }
            let name = if kind == fold::MIN { "min" } else { "max" };
            let msg = format!("{name}() iterable argument is empty");
            return Err(vm.err(T::ValueError, &msg));
        }
        fold::MIN | fold::MAX => st[0],
        _ => sum_value(vm, st[0], raw)?,
    }))
}

fn bi_sum(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let start = vm.arg_o(&a, 1);
    let acc = if start == Value::UNDEFINED {
        Value::int(0).unwrap_or(Value::NONE)
    } else {
        start
    };
    if Vm::is_str(acc) {
        return Err(vm.err(
            T::TypeError,
            "sum() can't sum strings [use ''.join(seq) instead]",
        ));
    }
    if Vm::is_bytes(acc) {
        return Err(vm.err(
            T::TypeError,
            "sum() can't sum bytes [use b''.join(seq) instead]",
        ));
    }
    let src = vm.arg_o(&a, 0);
    let itv = vm.get_iter(src)?;
    if src.is_obj_kind(ty::RANGE as u8)
        && let Some(start) = acc.as_int()
        && let Some(total) = vm.range_sum(itv).and_then(|s| s.checked_add(start.into()))
        && let Ok(total) = i64::try_from(total)
    {
        // A range sums in closed form, however long.
        return vm.new_int(total);
    }
    fold_start(vm, fold::SUM, itv, acc, Value::UNDEFINED)
}

/// Visits one native step of a long sort spends merging before it hands
/// the rest to `_sort_rest`, a slice per loop step, so a fuel limit,
/// deadline or park request is honoured between slices (docs/spec.md §5).
pub(crate) const SORT_SLICE: u64 = 1 << 17;

/// The visits a sort spends comparing str and other keys in one go (a
/// short sort, or a long sort's runs in one step) before the rest is left
/// to the prelude's `_sort_keys`, a comparison per safepoint ([`sort_lt`]).
const SORT_CAP: u32 = 1 << 20;

/// Items one native step of a long sort copies into its output at most.
const TAIL_SLICE: usize = 1 << 17;

/// A long sort first sorts runs of `1 << SORT_RUN_BITS` items in place, a
/// run at a time (even, so the merge passes start from the first array).
const SORT_RUN_BITS: usize = 10;

/// The pass number of a long sort's last step, which reverses (`reverse`)
/// or maps indexes to items (`key=`) in the merged array: `a`'s, or `b`'s.
const FINISH_A: usize = 62;
const FINISH_B: usize = 63;

/// How a sort's keys compare, found once per sort: all small ints, all
/// strs or all floats compare directly (CPython's pre-checked
/// `unsafe_long_compare`, `unsafe_latin_compare`, `unsafe_float_compare`);
/// anything else through [`Vm::order_pure`].
mod kind {
    pub const ANY: i64 = 0;
    pub const INTS: i64 = 1;
    pub const STRS: i64 = 2;
    pub const FLOATS: i64 = 3;
}

fn sort_kind(keys: &(impl Iterator<Item = Value> + Clone)) -> i64 {
    let all = |p: fn(Value) -> bool| keys.clone().all(p);
    if all(Value::is_int) {
        kind::INTS
    } else if all(Vm::is_str) {
        kind::STRS
    } else if all(Value::is_float) {
        kind::FLOATS
    } else {
        kind::ANY
    }
}

/// `x < y` for two keys of sort kind `kind`, counting its walk visits (a
/// str comparison one, and one per 64 bytes compared). Str and other keys
/// spend them from `left`, the budget of the whole sort or run: past it
/// the keys need Python ([`crate::ops::OrdErr::Py`]), so items that
/// compare long, such as big ints or long texts, cannot hold one step
/// (docs/spec.md §5).
#[inline]
fn sort_lt(
    vm: &Vm,
    kind: i64,
    x: Value,
    y: Value,
    visits: &mut u64,
    left: &mut u32,
) -> Result<bool, crate::ops::OrdErr> {
    match kind {
        kind::INTS => {
            *visits += 1;
            Ok(x.as_int() < y.as_int())
        }
        kind::STRS => {
            let (p, q) = (vm.str_bytes(x), vm.str_bytes(y));
            let n = 1 + p.len().min(q.len()) / 64;
            let Some(rest) = u32::try_from(n).ok().and_then(|n| left.checked_sub(n)) else {
                return Err(crate::ops::OrdErr::Py);
            };
            *left = rest;
            *visits += n as u64;
            Ok(p < q)
        }
        kind::FLOATS => {
            // NaN is unordered: `<` is false either way, as in Python.
            *visits += 1;
            Ok(x.as_float() < y.as_float())
        }
        _ => {
            let before = *left;
            let r = vm.order_pure(x, y, 0, left);
            // (Long texts past the budget, `text_over`, are taken once
            // per sort step by the caller; the next comparison has none.)
            *visits += u64::from(before - *left);
            r.map(|o| o == Some(Ordering::Less))
        }
    }
}

/// The key of `v` in a sort: `v` itself, or with `keys` the key at index `v`.
#[inline]
fn sort_key(keys: Option<&[u64]>, v: Value) -> Value {
    let Some(kw) = keys else { return v };
    let j = usize::try_from(v.as_int().unwrap_or(-1)).unwrap_or(usize::MAX);
    kw.get(j)
        .and_then(|&x| Value::from_bits(x))
        .unwrap_or(Value::NONE)
}

/// Sorts builtin-typed values with `<`, stably; `TypeError` if unorderable.
/// One native step: callers sort at most [`SORT_SLICE`] visits' worth this
/// way, a `capped` one at most [`SORT_CAP`] visits comparing them (past
/// it, [`Vm::need_py`]), or need no safepoint (json's `sort_keys`).
pub(crate) fn sort_values(
    vm: &mut Vm,
    items: &mut [Value],
    reverse: bool,
    capped: bool,
) -> Result<(), Raised> {
    let mut failure: Option<crate::ops::OrdErr> = None;
    let mut visits = 0;
    let mut left = if capped { SORT_CAP } else { u32::MAX };
    {
        let res = NativeValues::copy(&mut vm.heap, items);
        let mut buf = vm.hx_o(res)?;
        let vmr: &Vm = vm;
        let kind = sort_kind(&items.iter().copied());
        let mut lt = |x: &Value, y: &Value| -> bool {
            // After a failure the sort only finishes its passes.
            if failure.is_some() {
                return false;
            }
            match sort_lt(vmr, kind, *x, *y, &mut visits, &mut left) {
                Ok(b) => b,
                Err(e) => {
                    failure = Some(e);
                    false
                }
            }
        };
        merge_sort(items, &mut buf, reverse, &mut lt);
    }
    vm.burn(visits + vm.text_over.take());
    if let Some(e) = failure {
        return Err(vm.order_error(e, "<"));
    }
    Ok(())
}

/// Whether sorting `n` items may take more than one slice of visits: such
/// a sort runs as [`Vm::sort_by_keys`] slices.
pub(crate) fn sort_is_long(n: usize) -> bool {
    let n = n as u64;
    n.saturating_mul(u64::from(64 - n.leading_zeros())) > SORT_SLICE
}

/// What one slice of a sort came to.
pub(crate) enum Sorted {
    /// The sorted list.
    Done(Value),
    /// More slices to go, from the state list.
    More(Value),
    /// The keys do not order natively.
    Fail(crate::ops::OrdErr),
}

impl Vm {
    /// Starts a stable sort of list `xs` by the keys in list `keys` (as
    /// long; `xs` itself for a sort without `key=`): a bottom-up merge
    /// sort whose arrays and position live in a state list, so it can stop
    /// after any comparison. Runs the first slice.
    pub(crate) fn sort_by_keys(
        &mut self,
        xs: Value,
        keys: Value,
        reverse: bool,
    ) -> Result<Sorted, Raised> {
        let n = self.list_len(xs);
        let kind = sort_kind(
            &self
                .seq_words(keys)
                .iter()
                .map(|&w| Value::from_bits(w).unwrap_or(Value::NONE)),
        );
        // The merge's two arrays, of the items or of their indexes into
        // `keys`: pass 0 fills `a` a run at a time from `xs`, which the
        // sort never writes, so this step does no work per item. `b` comes
        // when the merge passes start (`sort_more`).
        let a = self.list_of_len(n)?;
        let b = Value::NONE;
        let int = |i: usize| Value::int(i as i64).unwrap_or(Value::NONE);
        let st = self.new_list(&[
            a,
            b,
            int(0),
            int(0),
            int(0),
            int(n.min(1)),
            int(0),
            Value::from_bool(reverse),
            Value::int(kind).unwrap_or(Value::NONE),
        ])?;
        self.sort_more(xs, keys, st)
    }

    /// One slice of the sort [`Vm::sort_by_keys`] started, from state list
    /// `st`: `[a, b, pass, i, l, r, k, reverse, kind]`, merging runs of
    /// `1 << pass` items of `a` (`b` on odd passes) into the other, the
    /// run pair at `i` with `l`, `r` and `k` its read and write positions;
    /// pass 0 sorts runs of `1 << SORT_RUN_BITS` items of `a` in place,
    /// from `i`.
    /// Stops after [`SORT_SLICE`] visits. The state is checked, never
    /// trusted: a mismatch is a `SystemError`.
    pub(crate) fn sort_more(
        &mut self,
        xs: Value,
        keys: Value,
        st: Value,
    ) -> Result<Sorted, Raised> {
        let w = self.seq_words(st);
        let get = |j: usize| {
            w.get(j)
                .and_then(|&x| Value::from_bits(x))
                .unwrap_or(Value::NONE)
        };
        let (a, b, rev, kind) = (
            get(0),
            get(1),
            get(7) == Value::from_bool(true),
            get(8).as_int().unwrap_or(-1),
        );
        let pos = |j: usize| get(j).as_int().and_then(|v| usize::try_from(v).ok());
        let n = self.list_len(xs);
        let keyed = keys != xs;
        let (Some(mut pass), Some(i), Some(l), Some(r), Some(k)) =
            (pos(2), pos(3), pos(4), pos(5), pos(6))
        else {
            return Err(self.bad_sort());
        };
        // `b` is `NONE` until the merge passes start, so while `a` holds
        // the items (an even pass).
        let no_b = b == Value::NONE && pass % 2 == 0 && (pass <= SORT_RUN_BITS || pass >= FINISH_A);
        if w.len() != 9
            || !a.is_obj_kind(ty::LIST as u8)
            || !(no_b || b.is_obj_kind(ty::LIST as u8) && self.list_len(b) == n)
            || !keys.is_obj_kind(ty::LIST as u8)
            || self.list_len(a) != n
            || self.list_len(keys) != n
            || !(kind::ANY..=kind::FLOATS).contains(&kind)
            || pass > FINISH_B
            || (pass != 0 && pass < SORT_RUN_BITS)
            || i.max(l).max(r).max(k) > n
            || !sort_at_ok(pass, [i, l, r, k], n, (rev, keyed))
            || b == a
            || [a, b].iter().any(|&t| t == xs || t == keys)
        {
            return Err(self.bad_sort());
        }
        let mut visits = 0u64;
        let mut at = [i, l, r, k];
        if pass == 0 {
            let mut left = SORT_CAP;
            // Runs of `1 << SORT_RUN_BITS` items sorted in place first,
            // each in one go: the merge passes over the arrays in the heap
            // start from there.
            let mut i = at[0];
            while i < n {
                if visits >= SORT_SLICE {
                    self.burn(visits);
                    let state = [
                        Value::int(0).unwrap_or(Value::NONE),
                        Value::int(i as i64).unwrap_or(Value::NONE),
                    ];
                    if let Some(buf) = self.list_buf(st) {
                        let res = self.heap.set_values(buf, 2, &state);
                        self.hx_o(res)?;
                    }
                    return Ok(Sorted::More(st));
                }
                let hi = (i + (1 << SORT_RUN_BITS)).min(n);
                // CPython's reverse sort: reverse, stable ascending sort,
                // reverse again (equal items keep their original order);
                // the first reversal is in where each run reads from.
                let at = |p: usize| if rev { n - 1 - p } else { p };
                let mut items: Vec<Value> = if keyed {
                    (i..hi)
                        .map(|p| Value::int(at(p) as i64).unwrap_or(Value::NONE))
                        .collect()
                } else {
                    let xw = self.seq_words(xs);
                    (i..hi)
                        .map(|p| {
                            xw.get(at(p))
                                .and_then(|&w| Value::from_bits(w))
                                .unwrap_or(Value::NONE)
                        })
                        .collect()
                };
                let mut buf = items.clone();
                let mut failure = None;
                {
                    let vm: &Vm = self;
                    let kw = keyed.then_some(vm.seq_words(keys));
                    let mut lt = |x: &Value, y: &Value| {
                        if failure.is_some() {
                            return false;
                        }
                        let (x, y) = (sort_key(kw, *x), sort_key(kw, *y));
                        match sort_lt(vm, kind, x, y, &mut visits, &mut left) {
                            Ok(b) => b,
                            Err(e) => {
                                failure = Some(e);
                                false
                            }
                        }
                    };
                    merge_sort(&mut items, &mut buf, false, &mut lt);
                }
                visits += self.text_over.take();
                if let Some(e) = failure {
                    self.burn(visits);
                    return Ok(Sorted::Fail(e));
                }
                if let Some(buf) = self.list_buf(a) {
                    let res = self.heap.set_values(buf, i as u32, &items);
                    self.hx_o(res)?;
                }
                i = hi;
            }
            pass = SORT_RUN_BITS;
            at = [0, 0, (1usize << pass).min(n), 0];
        }
        let mut b = b;
        // In u64: the finishing phases `FINISH_A` and `FINISH_B` are passes
        // past 31, which overflow `1usize << pass` on wasm32 (a panic with
        // overflow checks; a masked shift, by luck still `>= n`, without).
        if b == Value::NONE && (1u64 << pass) < n as u64 {
            // The second array comes at the start of a step of its own:
            // at up to 32 MiB, the system allocator may zero it as it
            // allocates it, as long a step as `a`'s.
            if visits > 0 {
                self.burn(visits);
                let int = |x: usize| Value::int(x as i64).unwrap_or(Value::NONE);
                let state = [int(pass), int(at[0]), int(at[1]), int(at[2]), int(at[3])];
                if let Some(buf) = self.list_buf(st) {
                    let res = self.heap.set_values(buf, 2, &state);
                    self.hx_o(res)?;
                }
                return Ok(Sorted::More(st));
            }
            b = self.list_of_len(n)?;
            if let Some(buf) = self.list_buf(st) {
                let res = self.heap.set_values(buf, 1, &[b]);
                self.hx_o(res)?;
            }
        }
        while (1u64 << pass) < n as u64 {
            let width = 1usize << pass;
            let (src, dst) = if pass % 2 == 0 { (a, b) } else { (b, a) };
            let k0 = at[3];
            // Room for what a slice writes in the common case, at once.
            let mut out = NativeValues::default();
            let res = out.reserve(&mut self.heap, (n - k0.min(n)).min(SORT_SLICE as usize));
            self.hx_o(res)?;
            let stop = {
                let vm: &Vm = self;
                let s = vm.seq_words(src);
                let kw = keyed.then_some(vm.seq_words(keys));
                let key = |v: Value| sort_key(kw, v);
                let (h, at, out, vs) = (&vm.heap, &mut at, &mut out, &mut visits);
                // One merge loop per key kind, each comparing directly.
                match kind {
                    kind::INTS => merge_slice(
                        h,
                        s,
                        width,
                        at,
                        out,
                        vs,
                        |v| key(v).as_int(),
                        |x, y, _| Ok(x < y),
                    ),
                    kind::FLOATS => merge_slice(
                        h,
                        s,
                        width,
                        at,
                        out,
                        vs,
                        |v| key(v).as_float(),
                        |x, y, _| Ok(x < y),
                    ),
                    kind::STRS => merge_slice(
                        h,
                        s,
                        width,
                        at,
                        out,
                        vs,
                        |v| vm.str_bytes(key(v)),
                        |x, y, vs| {
                            *vs += (x.len().min(y.len()) / 64) as u64;
                            Ok(x < y)
                        },
                    ),
                    _ => merge_slice(h, s, width, at, out, vs, key, |x, y, vs| {
                        let mut left = crate::dict::WALK;
                        let r = vm.order_pure(*x, *y, 0, &mut left);
                        *vs += u64::from(crate::dict::WALK - left).saturating_sub(1);
                        r.map(|o| o == Some(Ordering::Less))
                    }),
                }
            };
            // Long texts compared past a walk's budget (`spend_text`).
            visits += self.text_over.take();
            if let Merge::Oom = stop {
                let res: Result<(), _> = Err(mm_core::Error::OutOfMemory);
                self.hx_o(res)?;
            }
            if let Some(dst) = self.list_buf(dst) {
                let res = self.heap.set_values(dst, k0 as u32, &out);
                self.hx_o(res)?;
            }
            match stop {
                Merge::Fail(e) => {
                    self.burn(visits);
                    return Ok(Sorted::Fail(e));
                }
                Merge::Paused => {
                    // Out of visits part way through the pass: save where.
                    self.burn(visits);
                    let int = |x: usize| Value::int(x as i64).unwrap_or(Value::NONE);
                    let state = [int(pass), int(at[0]), int(at[1]), int(at[2]), int(at[3])];
                    if let Some(buf) = self.list_buf(st) {
                        let res = self.heap.set_values(buf, 2, &state);
                        self.hx_o(res)?;
                    }
                    return Ok(Sorted::More(st));
                }
                _ => {}
            }
            pass += 1;
            at = [0, 0, (1usize << pass).min(n), 0];
        }
        if pass < FINISH_A {
            pass = FINISH_A + pass % 2;
            at[0] = 0;
        }
        // The merged array is the result, in place: no second list of
        // garbage the size of the input. A reverse sort reverses it, and a
        // keyed one maps indexes to items, a slice per step.
        let done = if pass % 2 == 0 { a } else { b };
        let end = match (rev, keyed) {
            (false, false) => 0,
            (true, _) => n.div_ceil(2),
            (false, true) => n,
        };
        let (mut p, p0) = (at[0], at[0]);
        while p < end {
            // A slice of the merge, or of this pass, per step.
            if visits >= SORT_SLICE || p >= p0 + TAIL_SLICE {
                self.burn(visits);
                let int = |x: usize| Value::int(x as i64).unwrap_or(Value::NONE);
                if let Some(buf) = self.list_buf(st) {
                    let res = self.heap.set_values(buf, 2, &[int(pass), int(p)]);
                    self.hx_o(res)?;
                }
                return Ok(Sorted::More(st));
            }
            // Items `p..p + m` take items `n - 1 - p..` reversed
            // (`reverse`), mapped from indexes to items (`key=`), and the
            // other way round: read first, then written (where the halves
            // meet, both write the same middle item). A visit per 16.
            let m = (end - p).min(256).min(p0 + TAIL_SLICE - p);
            let (mut front, mut back) = ([Value::NONE; 256], [Value::NONE; 256]);
            {
                let (dw, xw) = (self.seq_words(done), self.seq_words(xs));
                let item = |w: u64| {
                    let v = Value::from_bits(w).unwrap_or(Value::NONE);
                    if !keyed {
                        return v;
                    }
                    let j = usize::try_from(v.as_int().unwrap_or(-1)).unwrap_or(usize::MAX);
                    xw.get(j)
                        .and_then(|&x| Value::from_bits(x))
                        .unwrap_or(Value::NONE)
                };
                for t in 0..m {
                    let q = if rev { n - 1 - p - t } else { p + t };
                    front[t] = item(dw.get(q).copied().unwrap_or(0));
                    if rev {
                        back[m - 1 - t] = item(dw.get(p + t).copied().unwrap_or(0));
                    }
                }
            }
            if let Some(buf) = self.list_buf(done) {
                if rev {
                    let res = self.heap.set_values(buf, (n - p - m) as u32, &back[..m]);
                    self.hx_o(res)?;
                }
                let res = self.heap.set_values(buf, p as u32, &front[..m]);
                self.hx_o(res)?;
            }
            visits += (m / 16) as u64;
            p += m;
        }
        self.burn(visits);
        Ok(Sorted::Done(done))
    }

    #[cold]
    fn bad_sort(&mut self) -> Raised {
        self.err(T::SystemError, "micro-monty: bad sort state")
    }
}

/// Whether `at` (`[i, l, r, k]`) is where a slice of pass `pass` of a
/// long sort of `n` items (`(reverse, key=)`) can stop: pass 0 at the
/// start of a run; a merge pass inside a run pair, its read positions in
/// their runs and the write position what they have read; a last pass
/// (`i` alone) before its end. Another position completed the sort
/// unsorted [M].
fn sort_at_ok(pass: usize, [i, l, r, k]: [usize; 4], n: usize, how: (bool, bool)) -> bool {
    if pass == 0 {
        return i % (1 << SORT_RUN_BITS) == 0;
    }
    if pass >= FINISH_A {
        let end = match how {
            (false, false) => 0,
            (true, _) => n.div_ceil(2),
            (false, true) => n,
        };
        return i < end;
    }
    let (Some(w), Some(n)) = (1u64.checked_shl(pass as u32), u64::try_from(n).ok()) else {
        return false;
    };
    let [i, l, r, k] = [i, l, r, k].map(|x| x as u64);
    let (mid, hi) = ((i + w).min(n), (i + 2 * w).min(n));
    w < n
        && i < n
        && i % (2 * w) == 0
        && (i..=mid).contains(&l)
        && (mid..=hi).contains(&r)
        && k + mid == l + r
}

/// How a slice of a long sort's merging stopped.
enum Merge {
    /// The pass is complete.
    Pass,
    /// Out of visits part way through.
    Paused,
    /// Two keys do not order natively.
    Fail(crate::ops::OrdErr),
    /// The output ran out of memory.
    Oom,
}

/// Merges the run pairs of `width` items of one pass of a long sort, its
/// source words `s`, from `at` (`[i, l, r, k]`: the pair's start, the read
/// positions, the write position) into `out` until the pass ends or
/// [`SORT_SLICE`] visits are spent. `key` prepares an item for `lt`, which
/// counts visits past the one each comparison costs. Generic so that each
/// key kind gets its own direct comparison loop.
#[allow(clippy::too_many_arguments, reason = "one merge step's state")]
fn merge_slice<P>(
    heap: &mm_core::Heap,
    s: &[u64],
    width: usize,
    at: &mut [usize; 4],
    out: &mut NativeValues,
    visits: &mut u64,
    key: impl Fn(Value) -> P,
    lt: impl Fn(&P, &P, &mut u64) -> Result<bool, crate::ops::OrdErr>,
) -> Merge {
    let n = s.len();
    let item = |j: usize| {
        s.get(j)
            .and_then(|&x| Value::from_bits(x))
            .unwrap_or(Value::NONE)
    };
    let [mut i, mut l, mut r, mut k] = *at;
    let mut skips = Skips::default();
    let stop = 'pass: loop {
        if i >= n {
            break Merge::Pass;
        }
        let mid = (i + width).min(n);
        let hi = (i + 2 * width).min(n);
        // Two runs already in order (sorted input) take one comparison.
        if l == i && r == mid && mid < hi && skips.worth() {
            *visits += 1;
            match lt(&key(item(mid)), &key(item(mid - 1)), visits).map(|b| skips.hit(!b)) {
                Err(e) => break Merge::Fail(e),
                Ok(false) => {}
                Ok(true) => {
                    for j in i..mid {
                        if out.push_shared(heap, item(j)).is_err() {
                            break 'pass Merge::Oom;
                        }
                    }
                    (l, k) = (mid, mid);
                }
            }
        }
        if l < mid && r < hi {
            // The two items compared next, decoded once, with their keys.
            let (mut xl, mut xr) = (item(l), item(r));
            let (mut kl, mut kr) = (key(xl), key(xr));
            loop {
                if *visits >= SORT_SLICE {
                    break 'pass Merge::Paused;
                }
                *visits += 1;
                let right = match lt(&kr, &kl, visits) {
                    Ok(b) => b,
                    Err(e) => break 'pass Merge::Fail(e),
                };
                if out.push_shared(heap, if right { xr } else { xl }).is_err() {
                    break 'pass Merge::Oom;
                }
                k += 1;
                if right {
                    r += 1;
                    if r == hi {
                        break;
                    }
                    xr = item(r);
                    kr = key(xr);
                } else {
                    l += 1;
                    if l == mid {
                        break;
                    }
                    xl = item(l);
                    kl = key(xl);
                }
            }
        }
        // The rest of the run left over, a visit per 16 items, at most a
        // slice's worth per step.
        while l < mid || r < hi {
            if out.len() >= TAIL_SLICE {
                break 'pass Merge::Paused;
            }
            let j = if l < mid { &mut l } else { &mut r };
            if out.push_shared(heap, item(*j)).is_err() {
                break 'pass Merge::Oom;
            }
            *j += 1;
            k += 1;
            *visits += u64::from(k % 16 == 0);
        }
        i = hi;
        (l, r, k) = (i, (i + width).min(n), i);
    };
    *at = [i, l, r, k];
    stop
}

/// Whether a merge pass still tries two runs for being in order already
/// (one comparison that saves the merge of sorted input): while at least
/// a quarter of the tries have paid, after the first eight.
#[derive(Default)]
struct Skips {
    tried: u32,
    paid: u32,
}

impl Skips {
    fn worth(&self) -> bool {
        self.tried < 8 || self.paid * 4 >= self.tried
    }

    fn hit(&mut self, in_order: bool) -> bool {
        self.tried += 1;
        self.paid += u32::from(in_order);
        in_order
    }
}

/// Stable merge sort calling only `lt`, as CPython's sort does; with
/// `reverse` the order is reversed while keeping stability.
fn merge_sort(
    items: &mut [Value],
    buf: &mut [Value],
    reverse: bool,
    lt: &mut dyn FnMut(&Value, &Value) -> bool,
) {
    if reverse {
        items.reverse();
    }
    let n = items.len();
    let mut width = 1;
    while width < n {
        let mut i = 0;
        let mut skips = Skips::default();
        while i < n {
            let mid = (i + width).min(n);
            let hi = (i + 2 * width).min(n);
            // Two runs already in order (sorted input) take one comparison.
            if mid < hi && skips.worth() && skips.hit(!lt(&items[mid], &items[mid - 1])) {
                buf[i..hi].copy_from_slice(&items[i..hi]);
                i += 2 * width;
                continue;
            }
            let (mut l, mut r, mut k) = (i, mid, i);
            while l < mid && r < hi {
                // CPython's reverse sort: reverse, stable ascending sort,
                // reverse again (equal items keep their original order).
                if lt(&items[r], &items[l]) {
                    buf[k] = items[r];
                    r += 1;
                } else {
                    buf[k] = items[l];
                    l += 1;
                }
                k += 1;
            }
            while l < mid {
                buf[k] = items[l];
                l += 1;
                k += 1;
            }
            while r < hi {
                buf[k] = items[r];
                r += 1;
                k += 1;
            }
            i += 2 * width;
        }
        items.copy_from_slice(buf);
        width *= 2;
    }
    if reverse {
        items.reverse();
    }
}

fn bi_sorted(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let key = vm.kwarg(&a, 0);
    let reverse = vm.kwarg(&a, 1);
    let reverse = reverse != Value::UNDEFINED && vm.truthy(reverse);
    let key = if key == Value::UNDEFINED {
        Value::NONE
    } else {
        key
    };
    let src = vm.arg_o(&a, 0);
    if vm.seq_items_len(src).is_some_and(sort_is_long) || key != Value::NONE {
        // A long sort without `key=` runs no Python code, so it reads a
        // list in place (it never writes it); otherwise a copy, as
        // CPython's: `key` may change the list.
        let xs = if key == Value::NONE && src.is_obj_kind(ty::LIST as u8) {
            src
        } else if vm.seq_items_len(src).is_some() {
            vm.seq_copy(src)?
        } else {
            let items = vm.collect(src)?;
            vm.new_list(&items)?
        };
        return vm.sort_start(xs, key, reverse, Value::NONE);
    }
    let mut items = vm.collect(src)?;
    if sort_is_long(items.len()) {
        let xs = vm.new_list(&items)?;
        return vm.sort_start(xs, key, reverse, Value::NONE);
    }
    sort_values(vm, &mut items, reverse, true)?;
    vm.new_list(&items)
}

fn bi_any(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    any_all(vm, a, true)
}

fn bi_all(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    any_all(vm, a, false)
}

/// `any` (`stop` true) and `all` (`stop` false): scans for an item whose
/// truth is `stop`, charging one unit per item. A container's iterator
/// folds in steps ([`fold_start`]). Over `enumerate`, `zip` and `reversed`
/// an item with a user `__bool__` or `__len__`, or the
/// [`crate::dict::WALK`]th item (they can be endless), continues in
/// prelude bytecode (`_any_rest`/`_all_rest`) from the same item and
/// iterator, so it parks (docs/spec.md §5).
fn any_all(vm: &mut Vm, a: CallArgs, stop: bool) -> Result<Value, Raised> {
    let src = vm.arg_o(&a, 0);
    let it = vm.get_iter(src)?;
    if it.is_obj_kind(ty::ITER as u8) {
        let kind = if stop { fold::ANY } else { fold::ALL };
        return fold_start(vm, kind, it, Value::UNDEFINED, Value::UNDEFINED);
    }
    if !Vm::is_native_iter(it) {
        return Err(vm.need_py());
    }
    let endless = it == src;
    let mut n = 0;
    let found = loop {
        let Some(x) = vm.iter_next(it)? else {
            break false;
        };
        n += 1;
        let pending = match vm.truthy_fast(x) {
            Some(t) if t == stop => break true,
            Some(_) if n < crate::dict::WALK || !endless => continue,
            Some(_) => Value::from_bool(!stop),
            None => x,
        };
        vm.burn(u64::from(n));
        let rest = if stop { "_any_rest" } else { "_all_rest" };
        return vm.prelude_request(rest, alloc::vec![pending, it], Cont::None);
    };
    vm.burn(u64::from(n));
    Ok(Value::from_bool(found == stop))
}

fn bi_hash(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if vm.uflags(v) & crate::class::f::HASH != 0
        && vm.dunder(v, Dn::Hash) != Some(Value::NONE)
        && let Some(r) = vm.dunder_call(v, Dn::Hash, &[], Cont::Check(check::HASH))?
    {
        return Ok(r);
    }
    let h = vm.hash(v)?;
    vm.new_int(h)
}

fn bi_id(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    match v.as_obj() {
        Some(r) => vm.new_int(0x7f00_0000_0000 + (i64::from(r.raw()) << 3)),
        // An immediate is its bits; above 2^64 so no object id collides.
        None => vm.int_from_i128((1 << 64) | i128::from(v.to_bits())),
    }
}

fn bi_iter(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if a.n > 1 {
        let (f, sentinel) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
        if !is_callable(vm, f) {
            return Err(vm.err(T::TypeError, "iter(v, w): v must be callable"));
        }
        let r = {
            let res = vm.heap.alloc_init(ty::CALLITER, 0, &[f, sentinel], &[]);
            vm.hx_o(res)
        }?;
        return Ok(Value::from_ref(r, ty::CALLITER as u8));
    }
    vm.op_get_iter(vm.arg_o(&a, 0))
}

fn bi_next(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let it = vm.arg_o(&a, 0);
    let d = vm.arg_o(&a, 1);
    if it.is_obj_kind(ty::GEN as u8) {
        return vm.call_request(it, alloc::vec![Value::NONE], Cont::Next(d));
    }
    if it.is_obj_kind(ty::CALLITER as u8) {
        if vm.vslot_o(it, 1) == Value::UNDEFINED {
            if d != Value::UNDEFINED {
                return Ok(d);
            }
            return Err(vm.raise_stop(Value::NONE));
        }
        let r = vm.calliter_next(it, u32::MAX)?;
        if d != Value::UNDEFINED
            && let Some(Request::Call { outer, .. }) = vm.request.as_mut()
        {
            *outer = Cont::Next(d);
        }
        return Ok(r);
    }
    if vm.user_class(it).is_some() {
        return vm.op_user_next(it, Cont::Next(d));
    }
    if let Some(v) = vm.iter_next(it)? {
        Ok(v)
    } else {
        let d = vm.arg_o(&a, 1);
        if d != Value::UNDEFINED {
            return Ok(d);
        }
        let e = vm.new_exception(T::StopIteration, &[])?;
        vm.exc = e;
        Err(Raised)
    }
}

fn bi_reversed(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if let Some(r) = vm.dunder_call(v, Dn::Reversed, &[], Cont::None)? {
        return Ok(r);
    }
    let b = vm.unbx(v);
    // A list or dict subclass inherits its base's `__reversed__`, which
    // reads the items whatever the subclass overrides; others reverse
    // through their `__getitem__` and `__len__`.
    if !matches!(vm.type_of(b), T::list | T::dict)
        && vm.uflags(v) & crate::class::f::GETITEM != 0
        && vm.uflags(v) & crate::class::f::LEN != 0
    {
        return vm.prelude_request("_seq_reversed", alloc::vec![v], Cont::None);
    }
    reversed_of(vm, b)
}

/// `list.__reversed__` and `dict.__reversed__`.
fn slot_reversed(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    reversed_of(vm, vm.unbx(vm.arg_o(&a, 0)))
}

/// The reverse iterator of builtin value `v`.
fn reversed_of(vm: &mut Vm, v: Value) -> Result<Value, Raised> {
    match vm.type_of(v) {
        T::list | T::tuple => {
            let n = vm.seq_words(v).len();
            let start = i64::try_from(n).unwrap_or(0) - 1;
            let r = {
                let res = vm
                    .heap
                    .alloc_init(ty::REVERSED, 0, &[v], &[start.cast_unsigned()]);
                vm.hx_o(res)
            }?;
            Ok(Value::from_ref(r, ty::REVERSED as u8))
        }
        T::range => vm.range_iter(v, true),
        T::str
        | T::dict
        | T::dict_keys
        | T::dict_values
        | T::dict_items
        | T::bytes
        | T::bytearray => {
            let mut items = vm.collect(v)?;
            items.reverse();
            let l = vm.new_list(&items)?;
            vm.get_iter(l)
        }
        _ => Err(vm.err(
            T::TypeError,
            &format!("'{}' object is not reversible", vm.type_name(v)),
        )),
    }
}

fn bi_round(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    let nd = vm.arg_o(&a, 1);
    let dargs: &[Value] = if nd == Value::UNDEFINED || nd == Value::NONE {
        &[]
    } else {
        &[nd]
    };
    if let Some(r) = vm.dunder_call(v, Dn::Round, dargs, Cont::None)? {
        return Ok(r);
    }
    let v = vm.unbx(v);
    let nd = if nd == Value::UNDEFINED || nd == Value::NONE {
        None
    } else {
        Some(vm.ssize_arg(nd)?)
    };
    if Vm::is_heap_int(v) {
        let b = vm.big_of(v).unwrap_or_default();
        let Some(n) = nd.filter(|&n| n < 0) else {
            return Ok(v);
        };
        // 10**k / 2 > |b| once k exceeds its bit length.
        if n.unsigned_abs() > b.bit_length() {
            return vm.new_int(0);
        }
        // `10 ** k`, the division by it and the product.
        let (k, lb) = (n.unsigned_abs(), b.mag.len() as u64);
        let lp = (k * 3322 / 1000).div_ceil(32) + 1;
        vm.work_room(
            crate::intops::div_ns(lb, lp)
                .saturating_add(crate::intops::mul_ns(lb.saturating_sub(lp) + 1, lp))
                .saturating_add(crate::intops::mul_ns(lp / 2 + 1, lp / 2 + 1).saturating_mul(2)),
        )?;
        let p = crate::bigint::Big::from_i64(10).pow(k);
        let (q, r) = b.divmod_floor(&p);
        let twice = r.shl(1);
        let up = match twice.cmp(&p) {
            Ordering::Greater => true,
            Ordering::Equal => q.bit(0),
            Ordering::Less => false,
        };
        let q = if up {
            q.add(&crate::bigint::Big::from_i64(1))
        } else {
            q
        };
        return vm.new_big(&q.mul(&p));
    }
    if let Some(i) = vm.int_of_o(v) {
        let Some(n) = nd else { return vm.new_int(i) };
        if n >= 0 {
            return vm.new_int(i);
        }
        let p = 10i128
            .checked_pow(u32::try_from(-n).unwrap_or(40))
            .unwrap_or(i128::MAX);
        let x = i128::from(i);
        let q = x.div_euclid(p);
        let r = x.rem_euclid(p);
        let q = if 2 * r > p || (2 * r == p && q % 2 != 0) {
            q + 1
        } else {
            q
        };
        return vm.int_from_i128(q * p);
    }
    let Some(f) = v.as_float() else {
        return Err(vm.err(
            T::TypeError,
            &format!("type {} doesn't define __round__ method", vm.type_name(v)),
        ));
    };
    match nd {
        None => {
            if !f.is_finite() {
                return Err(vm.err(
                    if f.is_nan() {
                        T::ValueError
                    } else {
                        T::OverflowError
                    },
                    if f.is_nan() {
                        "cannot convert float NaN to integer"
                    } else {
                        "cannot convert float infinity to integer"
                    },
                ));
            }
            // Half to even, then exactly: a float past 2**63 is an integer
            // already.
            let fl = crate::ops::floor(f);
            let diff = f - fl;
            let r = if diff > 0.5 || (diff == 0.5 && fl % 2.0 != 0.0) {
                fl + 1.0
            } else {
                fl
            };
            vm.new_big(&crate::bigint::Big::from_f64(r))
        }
        Some(n) => {
            // CPython's NDIGITS_MAX and NDIGITS_MIN.
            if !f.is_finite() || n > 323 {
                return Ok(v);
            }
            if n < -308 {
                return Ok(Value::float(0.0 * f));
            }
            let s = if n >= 0 {
                format!("{:.*}", usize::try_from(n).unwrap_or(0), f)
            } else {
                // Correctly rounded, as CPython's dtoa: the float's exact
                // value `mant * 2**e` to a multiple of 10**k, half to even.
                let bits = f.to_bits();
                let exp = ((bits >> 52) & 0x7FF) as i64;
                let (mant, e) = match exp {
                    0 => (bits & ((1 << 52) - 1), -1074),
                    _ => ((bits & ((1 << 52) - 1)) | (1 << 52), exp - 1075),
                };
                let p = crate::bigint::Big::from_i64(10).pow(n.unsigned_abs());
                let m = crate::bigint::Big::from_u64(mant, false);
                let (num, den) = if e >= 0 {
                    (m.shl(e.unsigned_abs()), p.clone())
                } else {
                    (m, p.shl(e.unsigned_abs()))
                };
                let (q, r) = num.divmod_floor(&den);
                let up = match r.shl(1).cmp(&den) {
                    Ordering::Greater => true,
                    Ordering::Equal => q.bit(0),
                    Ordering::Less => false,
                };
                let q = if up {
                    q.add(&crate::bigint::Big::from_i64(1))
                } else {
                    q
                };
                let Some(r) = q.mul(&p).to_f64() else {
                    return Err(vm.err(T::OverflowError, "rounded value too large to represent"));
                };
                // The sign survives a result of zero, as in CPython.
                return Ok(Value::float(crate::libm::copysign(r, f)));
            };
            Ok(Value::float(s.parse().unwrap_or(f)))
        }
    }
}

fn bi_divmod(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (x, y) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    let num = |v: Value| Vm::is_intlike(v) || v.is_float();
    if vm.user_class(x).is_some() || vm.user_class(y).is_some() {
        // User slots: the prelude twin (`binary_op1`).
        return Err(vm.need_py());
    }
    if !num(x) || !num(y) {
        return Err(vm.err(
            T::TypeError,
            &format!(
                "unsupported operand type(s) for divmod(): '{}' and '{}'",
                vm.type_name(x),
                vm.type_name(y)
            ),
        ));
    }
    if (Vm::is_heap_int(x) || Vm::is_heap_int(y))
        && let (Some(p), Some(q)) = (vm.big_of(x), vm.big_of(y))
        && !q.is_zero()
    {
        // One division, not two (docs/spec.md §5, bounded native steps).
        return vm.big_divmod(&p, &q);
    }
    let q = vm.binop(mm_compile::ast::BinOp::FloorDiv, x, y, false)?;
    let r = vm.binop(mm_compile::ast::BinOp::Mod, x, y, false)?;
    vm.alloc_tuple(&[q, r])
}

fn bi_pow(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (x, y, m) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1), vm.arg_o(&a, 2));
    if m == Value::UNDEFINED || m == Value::NONE {
        return vm.binop(mm_compile::ast::BinOp::Pow, x, y, false);
    }
    if [x, y, m].iter().any(|&v| vm.user_class(v).is_some()) {
        // User slots: the prelude twin.
        return Err(vm.need_py());
    }
    if let (Some(b), Some(e), Some(md)) = (vm.big_of(x), vm.big_of(y), vm.big_of(m)) {
        return vm.big_modpow(&b, &e, &md);
    }
    // CPython's `ternary_op` over the builtin slots, each slot once: int's
    // declines unless all are ints, float's rejects any modulus, complex's
    // rejects a modulus once both operands are numbers.
    let slot = |v: Value| {
        if Vm::is_intlike(v) {
            1
        } else if v.is_float() {
            2
        } else if Vm::is_complex(v) {
            3
        } else {
            0
        }
    };
    let sv = slot(x);
    let sw = if slot(y) == sv { 0 } else { slot(y) };
    let sz = if slot(m) == sv || slot(m) == sw {
        0
    } else {
        slot(m)
    };
    let num = |v: Value| Vm::is_number(v) || Vm::is_complex(v);
    for s in [sv, sw, sz] {
        match s {
            2 => {
                return Err(vm.err(
                    T::TypeError,
                    "pow() 3rd argument not allowed unless all arguments are integers",
                ));
            }
            3 if num(x) && num(y) => return Err(vm.err(T::ValueError, "complex modulo")),
            _ => {}
        }
    }
    let msg = format!(
        "unsupported operand type(s) for ** or pow(): '{}', '{}', '{}'",
        String::from(vm.type_name(x)),
        String::from(vm.type_name(y)),
        vm.type_name(m)
    );
    Err(vm.err(T::TypeError, &msg))
}

fn bi_chr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    // Any int out of range, however large, is a ValueError.
    let i = if Vm::is_heap_int(v) {
        -1
    } else {
        vm.int_arg(v, "chr")?
    };
    match u32::try_from(i).ok().and_then(char::from_u32) {
        Some(c) => {
            let mut b = [0u8; 4];
            vm.new_str(c.encode_utf8(&mut b))
        }
        None if (0xD800..=0xDFFF).contains(&i) => Err(vm.surrogate("chr()", i)),
        None => Err(vm.err(T::ValueError, "chr() arg not in range(0x110000)")),
    }
}

fn bi_ord(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let mut v = vm.arg_o(&a, 0);
    // A str or bytes subclass: its value.
    if let Some(b) = vm.backing(v)
        && (Vm::is_str(b) || Vm::is_bytes_like(b))
    {
        v = b;
    }
    if Vm::is_bytes_like(v) {
        return match *vm.bytes_data(v) {
            [c] => vm.new_int(i64::from(c)),
            ref d => {
                let msg = format!(
                    "ord() expected a character, but string of length {} found",
                    d.len()
                );
                Err(vm.err(T::TypeError, &msg))
            }
        };
    }
    if !Vm::is_str(v) {
        return Err(vm.err(
            T::TypeError,
            &format!(
                "ord() expected string of length 1, but {} found",
                vm.type_name(v)
            ),
        ));
    }
    let n = vm.str_len(v);
    if n != 1 {
        return Err(vm.err(
            T::TypeError,
            &format!("ord() expected a character, but string of length {n} found"),
        ));
    }
    let c = vm.str_ref_o(v).chars().next().map_or(0, |c| c as u32);
    vm.new_int(i64::from(c))
}

/// `bin` (`shift` 1), `oct` (3) and `hex` (4): [`Vm::pow2_text`], a
/// step at a time.
#[cfg_attr(mm_shipped, optimize(size))]
fn radix(vm: &mut Vm, a: &CallArgs, shift: u64) -> Result<Value, Raised> {
    let v = vm.arg_o(a, 0);
    // An int subclass (IntEnum, IntFlag) without its own `__index__`.
    let v = if vm.uflags(v) & crate::class::f::INDEX == 0 {
        vm.unbx(v)
    } else {
        v
    };
    if !Vm::is_heap_int(v) && vm.int_of_o(v).is_none() {
        vm.int_arg(v, "")?;
        return Err(vm.err(T::TypeError, "an integer is required"));
    }
    let letter = u64::from(b"box"[shift as usize / 2]);
    vm.pow2_text(v, shift + letter * pw::PREFIX, 0)
}

/// What a power-of-two digit text ([`Vm::pow2_text`]) holds, in one word:
/// the digit's bits (1, 3 or 4), uppercase, then a byte each for the
/// separator (every 4 digits), a non-negative int's sign, the
/// prefix letter, and, padded as a format, its fill char and alignment.
pub(crate) mod pw {
    pub const UPPER: u64 = 1 << 3;
    pub const SEP: u64 = 1 << 8;
    pub const SIGN: u64 = 1 << 16;
    pub const PREFIX: u64 = 1 << 24;
    pub const ALIGN: u64 = 1 << 32;
    pub const FILL: u64 = 1 << 40;
}

/// A `NUM_TEXT` state's kind for power-of-two digits, above `int()`'s
/// `1 + base` ([`Vm::digits_step`]).
pub(crate) const DIGITS: u64 = 64;

/// Digits a step writes (a few milliseconds); a fuel unit pays for 16.
const DIGIT_STEP: usize = 1 << 21;

impl Vm {
    /// The digits of int `v` in power-of-two radix, as `conf` says
    /// ([`pw`]), written from the end of one arena block that becomes the
    /// str: the int read in place, the first step in this call, the rest
    /// in `_steps`. With an alignment, then padded to `width`
    /// ([`Vm::num_pad`]).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn pow2_text(&mut self, v: Value, conf: u64, width: u64) -> Result<Value, Raised> {
        let shift = (conf & 7).max(1) as usize;
        let (m, neg) = self.magnitude(v);
        let n = m.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        let top = m.get(n.wrapping_sub(1)).map_or(0, |b| b.leading_zeros()) as usize;
        let digits = (n * 8).saturating_sub(top).div_ceil(shift).max(1);
        let sep = if conf >> 8 & 0xff != 0 {
            (digits - 1) / 4
        } else {
            0
        };
        // `conf` holds a non-negative int's sign (`+`, a space or none).
        let sign = if neg {
            u64::from(b'-')
        } else {
            conf >> 16 & 0xff
        };
        let conf = (conf & !(0xff * pw::SIGN)) + sign * pw::SIGN;
        let head = usize::from(sign != 0) + 2 * usize::from(conf >> 24 & 0xff != 0);
        let total = head + digits + sep;
        let block = {
            let res = self.heap.alloc_bytes_zeroed(total);
            self.hx_o(res)
        }?;
        self.collect_for_result(total);
        let mut w = [total as u64, 0, 0, 0, conf, width];
        if self.pow2_run(v, block, &mut w)? {
            return self.pow2_done(block, &w);
        }
        let raw = [DIGITS, w[0], w[1], w[2], w[3], w[4], w[5], 0, 0];
        let vals = [v, Value::from_byte_ref(block)];
        self.chunk_rest(nstep::op::NUM_TEXT, &vals, &raw)
    }

    /// The bytes of `v`'s magnitude (little-endian) and its sign.
    fn magnitude(&self, v: Value) -> (alloc::borrow::Cow<'_, [u8]>, bool) {
        if let Some(r) = v.as_obj().filter(|_| Vm::is_heap_int(v)) {
            let d = self.slot_o(r, 0).as_byte_ref();
            let m = d.and_then(|d| self.heap.bytes(d).ok()).unwrap_or(&[]);
            return (m.into(), self.raw_o(r, 1) == 1);
        }
        let i = self.int_of_o(v).unwrap_or(0);
        (i.unsigned_abs().to_le_bytes().to_vec().into(), i < 0)
    }

    /// One step of a power-of-two digit text ([`Vm::pow2_text`]).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn digits_step(
        &mut self,
        state: Value,
        v: Value,
        block: Value,
        at: u64,
        w: [u64; 5],
    ) -> Result<nstep::Step, Raised> {
        // An int's digits; any other value is a forged image.
        let Some(b) = block.as_byte_ref().filter(|_| Vm::is_intlike(v)) else {
            return Err(self.bad_step());
        };
        let mut w = [at, w[0], w[1], w[2], w[3], w[4]];
        if self.pow2_run(v, b, &mut w)? {
            return self.pow2_done(b, &w).map(nstep::Step::Done);
        }
        let [at, rest @ ..] = w;
        self.text_save(state, at as usize, rest, [0, 0]);
        Ok(nstep::Step::More)
    }

    /// Writes a step's digits of `v` into `block`, down from `w[0]`, the
    /// int's bytes read from `w[1]`, `w[2]` its bits not yet written and
    /// `w[3]` how many; then the sign and prefix. Whether it is done.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn pow2_run(
        &mut self,
        v: Value,
        block: mm_core::ByteRef,
        w: &mut [u64; 6],
    ) -> Result<bool, Raised> {
        let conf = w[4];
        let shift = (conf & 7) as u32;
        let (sep, sign, letter) = (conf >> 8 & 0xff, conf >> 16 & 0xff, conf >> 24 & 0xff);
        let head = usize::from(sign != 0) + 2 * usize::from(letter != 0);
        let total = self.heap.bytes(block).map_or(0, <[u8]>::len);
        let (mut at, mut done) = (w[0] as usize, w[1] as usize);
        let (mut acc, mut held) = (w[2], w[3] as u32);
        if !matches!(shift, 1 | 3 | 4) || at > total || at < head || held >= 8 {
            return Err(self.bad_step());
        }
        let digits: &[u8; 16] = if conf & pw::UPPER != 0 {
            b"0123456789ABCDEF"
        } else {
            b"0123456789abcdef"
        };
        let mask = (1u64 << shift) - 1;
        // Where the next char falls in its group of 4 digits and a
        // separator, counted from the right.
        let mut phase = (total - at) % 5;
        let end = at.saturating_sub(DIGIT_STEP).max(head);
        let mut buf = [0u8; 4096];
        // A byte's bits as `0`/`1` digits, high bit first.
        let spread = |b: u8| {
            let one = ((u64::from(b) * 0x0101_0101_0101_0101) & 0x8040_2010_0804_0201)
                + 0x7f7f_7f7f_7f7f_7f7f;
            ((one >> 7) & 0x0101_0101_0101_0101 | 0x3030_3030_3030_3030).to_be_bytes()
        };
        while at > end {
            let n = self.magnitude(v).0.len();
            // About a step's digits, a byte giving `8 / shift`.
            let len = n
                .saturating_sub(done)
                .min(buf.len())
                .min(((at - end) * shift as usize / 8).max(1));
            if let Some(from) = self.magnitude(v).0.get(done..done + len) {
                buf[..len].copy_from_slice(from);
            }
            let Ok(out) = self.heap.bytes_mut(block) else {
                return Err(self.bad_step());
            };
            let mut put = |at: &mut usize, d: u8| {
                // A separator first (to its left) once 4 digits are out.
                let k = usize::from(sep != 0 && phase == 4);
                for &c in &[sep as u8, d][1 - k..] {
                    *at = at.saturating_sub(1);
                    if let Some(o) = out.get_mut(*at) {
                        *o = c;
                    }
                }
                phase = if k == 1 { 1 } else { phase + 1 };
            };
            if len == 0 {
                // The last bits, or the digit of 0.
                while at > head {
                    put(&mut at, digits[(acc & mask) as usize]);
                    acc >>= shift;
                }
                break;
            }
            if sep == 0 {
                // No separator: digits straight into place.
                for (k, &b) in buf[..len].iter().enumerate() {
                    if shift == 1 && held == 0 && done + k + 1 < n && at >= head + 8 {
                        if let Some(o) = out.get_mut(at - 8..at) {
                            o.copy_from_slice(&spread(b));
                        }
                        at -= 8;
                        continue;
                    }
                    acc |= u64::from(b).checked_shl(held).unwrap_or(0);
                    held += 8;
                    while held >= shift && at > head {
                        at -= 1;
                        out[at] = digits[(acc & mask) as usize];
                        acc >>= shift;
                        held -= shift;
                    }
                }
                done += len;
                continue;
            }
            for (k, &b) in buf[..len].iter().enumerate() {
                if shift == 1 && held == 0 && done + k + 1 < n && at >= head + 10 {
                    // Eight digits a byte, high bit first, but for the top
                    // byte's leading zeros.
                    for &c in spread(b).iter().rev() {
                        put(&mut at, c);
                    }
                    continue;
                }
                acc |= u64::from(b).checked_shl(held).unwrap_or(0);
                held += 8;
                while held >= shift && at > head {
                    put(&mut at, digits[(acc & mask) as usize]);
                    acc >>= shift;
                    held -= shift;
                }
            }
            done += len;
        }
        self.burn((w[0] as usize).saturating_sub(at) as u64 / 16);
        *w = [at as u64, done as u64, acc, u64::from(held), conf, w[5]];
        if at > head {
            return Ok(false);
        }
        let lead = [sign as u8, b'0', letter as u8];
        let lead = &lead[usize::from(sign == 0)..=2 * usize::from(letter != 0)];
        if let Ok(out) = self.heap.bytes_mut(block)
            && let Some(p) = out.get_mut(..head)
        {
            p.copy_from_slice(lead);
        }
        Ok(true)
    }

    /// The finished digit text in `block`, padded when its `conf` has an
    /// alignment.
    fn pow2_done(&mut self, block: mm_core::ByteRef, w: &[u64; 6]) -> Result<Value, Raised> {
        let total = self.heap.bytes(block).map_or(0, <[u8]>::len);
        let res = self.heap.str_from_block(block, total as u64);
        let r = self.hx_o(res)?;
        let s = Value::from_ref(r, ty::STR as u8);
        let conf = w[4];
        let align = (conf >> 32 & 0xff) as u8;
        if align == 0 {
            return Ok(s);
        }
        let mut spec = crate::text::Spec::parse("").ok_or_else(|| self.bad_step())?;
        spec.fill = char::from_u32((conf >> 40) as u32).unwrap_or(' ');
        spec.align = Some(char::from(align));
        spec.width = usize::try_from(w[5]).unwrap_or(usize::MAX);
        spec.grouping = (conf >> 8 & 0xff != 0).then_some('_');
        spec.ty = Some('x');
        let sign = usize::from(conf >> 16 & 0xff != 0);
        let head = sign + 2 * usize::from(conf >> 24 & 0xff != 0);
        self.num_pad(s, head, total - head, &spec)
    }
}

fn bi_bin(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    radix(vm, &a, 1)
}

fn bi_hex(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    radix(vm, &a, 4)
}

fn bi_oct(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    radix(vm, &a, 3)
}

/// `callable(v)`.
pub(crate) fn is_callable(vm: &Vm, v: Value) -> bool {
    matches!(
        vm.type_of(v),
        T::function | T::builtin | T::type_ | T::method | T::staticmethod
    ) || vm.uflags(v) & crate::class::f::CALL != 0
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn bi_callable(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    Ok(Value::from_bool(is_callable(vm, v)))
}

fn bi_format(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let spec = vm.arg_o(&a, 1);
    let v = vm.arg_o(&a, 0);
    let spec_v = if spec == Value::UNDEFINED {
        vm.intern("")?
    } else if !Vm::is_str(vm.unbx(spec)) {
        let tn = if spec == Value::NONE {
            "None"
        } else {
            vm.type_name(spec)
        };
        let msg = format!("format() argument 2 must be str, not {tn}");
        return Err(vm.err(T::TypeError, &msg));
    } else {
        spec
    };
    if vm.uflags(v) & crate::class::f::FORMAT != 0
        && let Some(r) = vm.dunder_call(v, Dn::Format, &[spec_v], Cont::Check(check::FORMAT))?
    {
        return Ok(r);
    }
    vm.format_str(v, FormatSpec::Value(spec_v))
}

fn bi_getattr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (o, n, d) = (vm.arg_o(&a, 0), vm.unbx(vm.arg_o(&a, 1)), vm.arg_o(&a, 2));
    if !Vm::is_str(n) {
        return Err(vm.err(
            T::TypeError,
            &format!("attribute name must be string, not '{}'", vm.type_name(n)),
        ));
    }
    // Attribute names compare by identity: intern computed ones.
    let n = vm.intern_value(n)?;
    match vm.getattr(o, n) {
        Ok(v) if v == Value::UNDEFINED && d != Value::UNDEFINED => {
            vm.wrap_request(Cont::AttrDefault(d));
            Ok(v)
        }
        Ok(v) => Ok(v),
        Err(Raised) if d != Value::UNDEFINED && vm.exc_type(vm.exc) == T::AttributeError => Ok(d),
        Err(e) => Err(e),
    }
}

fn bi_setattr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (o, n, v) = (vm.arg_o(&a, 0), vm.unbx(vm.arg_o(&a, 1)), vm.arg_o(&a, 2));
    if !Vm::is_str(n) {
        let msg = format!("attribute name must be string, not '{}'", vm.type_name(n));
        return Err(vm.err(T::TypeError, &msg));
    }
    // Attribute names compare by identity: intern computed ones.
    let n = vm.intern_value(n)?;
    vm.setattr(o, n, v)?;
    Ok(if vm.request.is_some() {
        Value::UNDEFINED
    } else {
        Value::NONE
    })
}

fn bi_delattr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (o, n) = (vm.arg_o(&a, 0), vm.unbx(vm.arg_o(&a, 1)));
    if !Vm::is_str(n) {
        let msg = format!("attribute name must be string, not '{}'", vm.type_name(n));
        return Err(vm.err(T::TypeError, &msg));
    }
    // Attribute names compare by identity: intern computed ones.
    let n = vm.intern_value(n)?;
    vm.delattr(o, n)?;
    Ok(if vm.request.is_some() {
        Value::UNDEFINED
    } else {
        Value::NONE
    })
}

fn bi_issubclass(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (c, t) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if let Some(r) = vm.meta_check(c, t, Dn::Subclasscheck)? {
        return Ok(r);
    }
    if !Vm::is_type(c) {
        return Err(vm.err(T::TypeError, "issubclass() arg 1 must be a class"));
    }
    match any_class(
        vm,
        t,
        Dn::Subclasscheck,
        "issubclass() arg 2 must be a class, a tuple of classes, or a union",
        &|vm, x| vm.subclass_of(c, x),
    )? {
        Some(r) => Ok(Value::from_bool(r)),
        None => vm.prelude_request("_issubclass_any", alloc::vec![c, t], Cont::None),
    }
}

fn bi_property(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let mut parts = [Value::NONE; 4];
    for (i, p) in parts.iter_mut().enumerate() {
        let v = vm.arg_o(&a, u32::try_from(i).unwrap_or(0));
        if v != Value::UNDEFINED {
            *p = v;
        }
    }
    if parts[3] == Value::NONE && parts[0].is_obj_kind(ty::FUNC as u8) {
        // The getter's docstring.
        let k = vm.intern("__doc__")?;
        parts[3] = vm.getattr(parts[0], k)?;
    }
    let r = {
        let res = vm.heap.alloc_init(ty::PROPERTY, 0, &parts, &[]);
        vm.hx_o(res)
    }?;
    Ok(Value::from_ref(r, ty::PROPERTY as u8))
}

fn bi_staticmethod(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.wrap_descr(ty::STATICMETHOD, vm.arg_o(&a, 0))
}

fn bi_classmethod(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.wrap_descr(ty::CLASSMETHOD, vm.arg_o(&a, 0))
}

fn bi_super(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if a.n == 0 {
        return Err(vm.err(T::RuntimeError, "super(): no arguments"));
    }
    if a.n == 1 {
        return Err(vm.err(
            T::RuntimeError,
            "micro-monty does not support unbound super objects",
        ));
    }
    vm.new_super(vm.arg_o(&a, 0), vm.arg_o(&a, 1))
}

fn prop_copy(vm: &mut Vm, a: &CallArgs, i: u32) -> Result<Value, Raised> {
    let p = vm.arg_o(a, 0);
    let mut parts = [Value::NONE; 4];
    for (k, x) in parts.iter_mut().enumerate() {
        *x = vm.vslot_o(p, u32::try_from(k).unwrap_or(0));
    }
    parts[i as usize] = vm.arg_o(a, 1);
    let r = {
        let res = vm.heap.alloc_init(ty::PROPERTY, 0, &parts, &[]);
        vm.hx_o(res)
    }?;
    Ok(Value::from_ref(r, ty::PROPERTY as u8))
}

/// `__get__(obj, type=None)` of functions, classmethods, staticmethods and
/// properties: what attribute lookup binds (`None` for `obj` is the class
/// access, which returns a function or property itself).
fn descr_get(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (d, obj, cls) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1), vm.arg_o(&a, 2));
    if obj == Value::NONE && (cls == Value::NONE || cls == Value::UNDEFINED) {
        return Err(vm.err(T::TypeError, "__get__(None, None) is invalid"));
    }
    let cls = if cls == Value::UNDEFINED || cls == Value::NONE {
        vm.class_of(obj)
    } else {
        cls
    };
    if obj == Value::NONE && !d.is_obj_kind(ty::CLASSMETHOD as u8) {
        return Ok(if d.is_obj_kind(ty::STATICMETHOD as u8) {
            vm.vslot_o(d, 0)
        } else {
            d
        });
    }
    vm.bind_attr(d, obj, cls)
}

fn prop_getter(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    prop_copy(vm, &a, 0)
}

fn prop_setter(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    prop_copy(vm, &a, 1)
}

fn prop_deleter(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    prop_copy(vm, &a, 2)
}

// ----------------------------------------------------------- object

fn object_init(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let me = vm.arg_o(&a, 0);
    let kw = vm.kwarg(&a, 0);
    let extra = a.n > 1 || (kw != Value::UNDEFINED && vm.dict_len(kw) > 0);
    if extra {
        let c = vm.class_of(me);
        let init = vm.dn(Dn::Init)?;
        let new = vm.dn(Dn::New)?;
        let own_init = vm
            .class_lookup(c, init)
            .is_some_and(|f| f.as_native().is_none());
        let own_new = vm
            .class_lookup(c, new)
            .is_some_and(|f| f.as_native().is_none());
        // CPython's `object_init`: a class with its own `__init__` names
        // `object`, one with neither names itself.
        if own_init || !own_new {
            let msg = format!(
                "{}.__init__() takes exactly one argument (the instance to initialize)",
                if own_init { "object" } else { vm.class_name(c) }
            );
            return Err(vm.err(T::TypeError, &msg));
        }
    }
    Ok(Value::NONE)
}

// ------------------------------------------------- slot wrappers

fn slot_getitem(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.getitem(vm.arg_o(&a, 0), vm.arg_o(&a, 1))
}

fn slot_setitem(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.setitem(vm.arg_o(&a, 0), vm.arg_o(&a, 1), vm.arg_o(&a, 2))?;
    Ok(stored(vm))
}

fn slot_delitem(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.delitem(vm.arg_o(&a, 0), vm.arg_o(&a, 1))?;
    Ok(stored(vm))
}

/// The value of a store or deletion: `None`, or the request marker when
/// it goes on in steps (`Vm::setitem` on a long list).
fn stored(vm: &Vm) -> Value {
    if vm.request.is_some() {
        Value::UNDEFINED
    } else {
        Value::NONE
    }
}

fn slot_len(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.len_of(vm.arg_o(&a, 0))?;
    vm.new_int(i64::try_from(n).unwrap_or(i64::MAX))
}

fn slot_contains(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let r = vm.contains(vm.arg_o(&a, 0), vm.arg_o(&a, 1))?;
    Ok(Value::from_bool(r))
}

fn slot_iter(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.get_iter(vm.arg_o(&a, 0))
}

fn slot_repr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.repr(vm.arg_o(&a, 0))?;
    vm.new_str(&s)
}

/// `str.__str__`: the text itself, a `str` even for a subclass instance.
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn str_str(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    Ok(vm.unbx(vm.arg_o(&a, 0)))
}

fn slot_hash(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let h = vm.hash(vm.arg_o(&a, 0))?;
    vm.new_int(h)
}

/// A rich comparison slot: `NotImplemented` unless both operands are of
/// the receiver's builtin type.
fn slot_cmp(vm: &mut Vm, a: CallArgs, opcode: u8) -> Result<Value, Raised> {
    let (x, y) = (vm.arg_o(&a, 0), vm.unbx(vm.arg_o(&a, 1)));
    let (tx, ty_) = (vm.type_of(x), vm.type_of(y));
    let same = tx == ty_
        || matches!(
            (tx, ty_),
            (T::int, T::bool)
                | (T::bool, T::int)
                | (T::float, T::int | T::bool)
                | (T::set, T::frozenset)
                | (T::frozenset, T::set)
        );
    if !same {
        return Ok(Value::NOT_IMPLEMENTED);
    }
    vm.compare(opcode, x, y)
}

fn slot_eq(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    slot_cmp(vm, a, mm_compile::bytecode::op::TEST_EQ)
}

fn slot_ne(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    slot_cmp(vm, a, mm_compile::bytecode::op::TEST_NE)
}

fn slot_lt(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    slot_cmp(vm, a, mm_compile::bytecode::op::TEST_LT)
}

fn slot_le(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    slot_cmp(vm, a, mm_compile::bytecode::op::TEST_LE)
}

fn slot_gt(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    slot_cmp(vm, a, mm_compile::bytecode::op::TEST_GT)
}

fn slot_ge(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    slot_cmp(vm, a, mm_compile::bytecode::op::TEST_GE)
}

fn list_init(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let l = vm.check_self(&a, T::list, "__init__")?;
    let src = vm.arg_o(&a, 1);
    // CPython's `list___init___impl`: emptied, then extended, so the list
    // read as its own source is empty. A builtin iterator or a generator,
    // which can see the list grow, is read by the prelude twin.
    if Vm::one_shot(src) || src.is_obj_kind(ty::GEN as u8) {
        return Err(vm.need_py());
    }
    vm.list_set_items(l, &[])?;
    let items = if src == Value::UNDEFINED {
        NativeValues::default()
    } else {
        vm.collect(src)?
    };
    vm.list_set_items(l, &items)?;
    Ok(Value::NONE)
}

fn set_init(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.check_self(&a, T::set, "__init__")?;
    let src = vm.arg_o(&a, 1);
    let items = if src == Value::UNDEFINED {
        NativeValues::default()
    } else {
        vm.collect(src)?
    };
    for x in vm.set_items_charged(s) {
        vm.set_discard(s, x)?;
    }
    for &x in &items {
        vm.set_add_plain(s, x)?;
    }
    Ok(Value::NONE)
}

/// `T.__new__(cls, *args, **kwargs)` for a builtin type `T` that classes
/// may extend: an instance of `cls` extending `T(*args, **kwargs)` (an
/// empty value for the mutable types, which `__init__` fills).
fn bx_type_new(vm: &mut Vm, a: CallArgs, t: T) -> Result<Value, Raised> {
    let cls = vm.arg_o(&a, 0);
    let tobj = vm.type_obj(t);
    if !Vm::is_type(cls) {
        let msg = format!(
            "{}.__new__(X): X is not a type object ({})",
            t.name(),
            String::from(vm.type_name(cls))
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    if cls != tobj && !(Vm::is_class(cls) && vm.subclass_of(cls, tobj)) {
        let msg = format!(
            "{}.__new__({}): {} is not a subtype of {}",
            t.name(),
            String::from(vm.class_name(cls)),
            String::from(vm.class_name(cls)),
            t.name()
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    let mutable = matches!(t, T::list | T::dict | T::set);
    if mutable && cls != tobj {
        let empty = match t {
            T::list => vm.new_list(&[])?,
            T::dict => vm.new_dict()?,
            _ => vm.new_set()?,
        };
        return vm.new_bx(cls, empty);
    }
    let rest: Vec<Value> = (1..a.n).map(|i| vm.arg_o(&a, i)).collect();
    let kw = match vm.kwarg(&a, 0) {
        Value::UNDEFINED => Value::NONE,
        d => d,
    };
    let cont = if cls == tobj {
        Cont::None
    } else {
        Cont::BxWrap(cls)
    };
    vm.request = Some(Request::Call {
        func: tobj,
        args: rest.into(),
        kw,
        cont,
        outer: Cont::None,
    });
    Ok(Value::UNDEFINED)
}

fn tuple_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    bx_type_new(vm, a, T::tuple)
}

fn list_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    bx_type_new(vm, a, T::list)
}

fn dict_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    bx_type_new(vm, a, T::dict)
}

fn set_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    bx_type_new(vm, a, T::set)
}

fn int_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    bx_type_new(vm, a, T::int)
}

fn str_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    bx_type_new(vm, a, T::str)
}

fn float_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    bx_type_new(vm, a, T::float)
}

fn bytes_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    bx_type_new(vm, a, T::bytes)
}

/// `_bx_base(cls)`: the builtin type class `cls` extends.
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn pl_bx_base(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let t = vm.class_base_t(vm.arg_o(&a, 0));
    Ok(vm.type_obj(t))
}

/// `_bx_init(cls)`: the `__init__` Python code defines for `cls`, or `None`.
fn pl_bx_init(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let c = vm.arg_o(&a, 0);
    let k = vm.dn(Dn::Init)?;
    Ok(vm.user_lookup(c, k).unwrap_or(Value::NONE))
}

/// `_bx_make(cls, value)`: an instance of `cls` extending `value`.
fn pl_bx_make(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.new_bx(vm.arg_o(&a, 0), vm.arg_o(&a, 1))
}

fn object_new(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let c = vm.arg_o(&a, 0);
    if Vm::is_class(c) {
        let rest: Vec<Value> = (1..a.n).map(|i| vm.arg_o(&a, i)).collect();
        if vm.class_base_t(c).is_exception() {
            // `BaseException.__new__` keeps the arguments.
            return vm.alloc_of_class(c, &rest);
        }
        let kw = vm.kwarg(&a, 0);
        let excess = !rest.is_empty() || (kw != Value::UNDEFINED && vm.dict_len(kw) > 0);
        if excess {
            let fl = c
                .as_obj()
                .map_or(0, |r| vm.raw_o(r, crate::class::cls::FLAGS));
            let msg = if fl & crate::class::f::NEW != 0 {
                String::from(
                    "object.__new__() takes exactly one argument (the type to instantiate)",
                )
            } else if fl & crate::class::f::INIT == 0 {
                format!("{}() takes no arguments", vm.class_name(c))
            } else {
                String::new()
            };
            if !msg.is_empty() {
                return Err(vm.err(T::TypeError, &msg));
            }
        }
        return vm.alloc_of_class(c, &[]);
    }
    if c == vm.type_obj(T::object) {
        let o = vm.type_obj(T::object);
        return vm.new_instance(o, 0);
    }
    let msg = format!(
        "object.__new__(X): X is not a type object ({})",
        vm.type_name(c)
    );
    Err(vm.err(T::TypeError, &msg))
}

fn object_repr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    let s = if v.is_obj_kind(ty::EXC as u8) {
        let args = vm.vslot_o(v, crate::vm::exc_slot::ARGS);
        let n = args.as_obj().map_or(0, |r| vm.obj_len(r));
        for i in 0..n {
            let x = vm.vslot_o(args, i);
            if vm.text_needs_py(x) {
                return Err(vm.need_py());
            }
        }
        let mut out = NativeText::default();
        let len = vm.type_name(v).len();
        out.reserve(vm, len)?;
        out.text.push_str(vm.type_name(v));
        if n == 1 {
            out.push(vm, "(")?;
            let text = vm.repr(vm.vslot_o(args, 0))?;
            out.push(vm, &text)?;
            drop(text);
            out.push(vm, ")")?;
        } else {
            let text = vm.repr(args)?;
            out.push(vm, &text)?;
        }
        out
    } else if v.is_obj_kind(ty::INST as u8) {
        let c = vm.class_of(v);
        let name = vm.class_repr_name(c);
        let addr = 0x7f00_0000_0000_u64 + (u64::from(v.as_obj().map_or(0, mm_core::Ref::raw)) << 3);
        let mut out = NativeText::default();
        out.push(vm, "<")?;
        out.push(vm, &name)?;
        out.push(vm, " object at 0x")?;
        out.push(vm, &format!("{addr:x}"))?;
        out.push(vm, ">")?;
        out
    } else {
        vm.repr(v)?
    };
    vm.new_str(&s)
}

fn object_str(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if let Some(r) = vm.dunder_call(v, Dn::Repr, &[], Cont::Check(check::REPR))? {
        return Ok(r);
    }
    // `repr(v)`: a builtin-type subclass reprs as its value.
    bi_repr(vm, a)
}

fn object_format(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (v, spec) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    // A str subclass spec is read as its text.
    let spec = if Vm::is_str(vm.unbx(spec)) {
        vm.unbx(spec)
    } else {
        spec
    };
    // `(1).__format__(spec)`: the builtin types without their own
    // `__format__` entry format as `format()` does; an empty spec is
    // `str(v)`, which a subclass may override.
    let fl = vm.uflags(v);
    let own_str = vm.str_ref_o(spec).is_empty()
        && (fl & crate::class::f::STR != 0
            || fl & crate::class::f::REPR != 0 && !Vm::is_str(vm.unbx(v)));
    if Vm::is_str(spec)
        && !own_str
        && matches!(
            vm.type_of(vm.unbx(v)),
            T::int | T::bool | T::float | T::complex | T::str
        )
    {
        // A str subclass instance itself, which a no-op spec gives back.
        let v = if Vm::is_str(vm.unbx(v)) {
            v
        } else {
            vm.unbx(v)
        };
        return vm.format_str(v, FormatSpec::Value(spec));
    }
    if Vm::is_str(spec) && !vm.str_ref_o(spec).is_empty() {
        let msg = format!(
            "unsupported format string passed to {}.__format__",
            vm.type_name(v)
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    if vm.uflags(v) & (crate::class::f::STR | crate::class::f::REPR) != 0 {
        let m = if vm.uflags(v) & crate::class::f::STR != 0 {
            Dn::Str
        } else {
            Dn::Repr
        };
        if let Some(r) = vm.dunder_call(v, m, &[], Cont::Check(check::STR))? {
            return Ok(r);
        }
    }
    let s = vm.stringify(v)?;
    vm.new_str(&s)
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn object_eq(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    Ok(if vm.arg_o(&a, 0) == vm.arg_o(&a, 1) {
        Value::TRUE
    } else {
        Value::NOT_IMPLEMENTED
    })
}

fn object_ne(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (x, y) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if let Some(m) = vm.user_method(x, Dn::Eq) {
        let n = vm.call_dunder(
            m,
            x,
            &[y],
            Cont::Cmp {
                op: mm_compile::bytecode::op::TEST_NE,
                stage: 0x80 | 0x7f,
                a: x,
                b: y,
            },
        )?;
        return vm.next_request(n);
    }
    Ok(if x == y {
        Value::FALSE
    } else {
        Value::NOT_IMPLEMENTED
    })
}

fn object_hash(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    let bits = v.to_bits();
    vm.new_int(((bits >> 4) ^ (bits << 20)).cast_signed() >> 2)
}

fn object_setattr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (o, n, v) = (vm.arg_o(&a, 0), vm.unbx(vm.arg_o(&a, 1)), vm.arg_o(&a, 2));
    if !Vm::is_str(n) {
        let msg = format!("attribute name must be string, not '{}'", vm.type_name(n));
        return Err(vm.err(T::TypeError, &msg));
    }
    if vm.user_class(o).is_some() {
        vm.generic_setattr(o, n, v)?;
    } else if Vm::is_class(o) {
        vm.class_setattr(o, n, v)?;
    } else {
        vm.setattr(o, n, v)?;
    }
    Ok(if vm.request.is_some() {
        Value::UNDEFINED
    } else {
        Value::NONE
    })
}

fn object_getattribute(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (o, n) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    vm.getattr(o, n)
}

fn object_delattr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (o, n) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if vm.user_class(o).is_some() {
        vm.generic_delattr(o, n)?;
    } else if Vm::is_class(o) {
        vm.class_delattr(o, n)?;
    } else {
        vm.delattr(o, n)?;
    }
    Ok(if vm.request.is_some() {
        Value::UNDEFINED
    } else {
        Value::NONE
    })
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn type_mro(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let c = vm.arg_o(&a, 0);
    let m = vm.mro_of(c);
    vm.new_list(&m)
}

fn bi_vars(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let o = vm.arg_o(&a, 0);
    let k = vm.intern("__dict__")?;
    match vm.getattr(o, k) {
        Err(Raised) if vm.exc_type(vm.exc) == T::AttributeError => {
            Err(vm.err(T::TypeError, "vars() argument must have __dict__ attribute"))
        }
        r => r,
    }
}

/// `dir()` and `dir(o)`. `dir(o)` is the prelude's `_dir`: the `__dir__`
/// of `o`'s type (`object.__dir__`, `type.__dir__` and `module.__dir__`
/// are prelude code too). Without an argument it sorts the module globals
/// or the scope a compiled `dir()` passes as `<scope>`: a class namespace,
/// or the variable names of its caller's registers.
fn bi_dir(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if a.n > 0 {
        return Err(vm.need_py());
    }
    let scope = vm.kwarg(&a, 0);
    let names = if scope.is_obj_kind(ty::TUPLE as u8) {
        // Registers at or above the argument window (the caller's
        // temporaries, or past its frame) are never read.
        let base = vm.frame_base;
        let mut bound = Vec::new();
        for (i, n) in vm.tuple_items(scope).into_iter().enumerate() {
            if base + i + crate::vm::HDR >= a.base as usize {
                break;
            }
            let v = vm.stack.get(base + i).copied().unwrap_or(Value::UNDEFINED);
            let v = if v.is_obj_kind(ty::CELL as u8) {
                vm.vslot_o(v, 0)
            } else {
                v
            };
            // Hidden compiler names (`.0`) are not variables.
            if v != Value::UNDEFINED && Vm::is_str(n) && !vm.str_ref_o(n).starts_with('.') {
                bound.push(n);
            }
        }
        vm.new_list(&bound)?
    } else if scope.is_obj_kind(ty::DICT as u8) {
        scope
    } else {
        vm.globals
    };
    vm.prelude_request("_dir_sort", alloc::vec![names], Cont::None)
}

fn object_init_subclass(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let kw = vm.kwarg(&a, 0);
    if kw != Value::UNDEFINED && kw != Value::NONE && vm.dict_len(kw) > 0 {
        let c = vm.arg_o(&a, 0);
        let msg = format!(
            "{}.__init_subclass__() takes no keyword arguments",
            String::from(vm.class_name(c))
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    Ok(Value::NONE)
}

// ------------------------------------------------------- exceptions

fn exc_init(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = vm.arg_o(&a, 0);
    let rest: Vec<Value> = (1..a.n).map(|i| vm.arg_o(&a, i)).collect();
    let t = vm.alloc_tuple(&rest)?;
    if let Some(r) = e.as_obj().filter(|_| e.is_obj_kind(ty::EXC as u8)) {
        vm.set_slot_o(r, crate::vm::exc_slot::ARGS, t);
        // `NameError`, `AttributeError` and `ImportError` take keywords
        // (`super().__init__(msg, name=...)`); the others refuse them,
        // naming the instance's class as CPython does.
        let kw = vm.kwarg(&a, 0);
        if kw.is_obj_kind(ty::DICT as u8) && vm.dict_len(kw) > 0 {
            let kws = vm.dict_items_vec(kw);
            let c = vm.class_of(e);
            let name = String::from(vm.class_name(c));
            vm.exc_init_kwargs(e, &name, &kws)?;
        }
    }
    Ok(Value::NONE)
}

fn exc_str_m(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = vm.arg_o(&a, 0);
    if let Some(s) = vm.syntax_str(e)? {
        return vm.new_str(&s);
    }
    let args = vm.vslot_o(e, crate::vm::exc_slot::ARGS);
    let n = vm.tuple_len(args);
    let os_error = vm.exc_type(e).is_subclass(T::OSError);
    let s = match n {
        0 => NativeText::default(),
        // `[Errno 2] No such file or directory: 'name' -> 'name2'`
        2..=5 if os_error => {
            let no = vm.vslot_o(args, 0);
            let msg = vm.vslot_o(args, 1);
            let mut s = NativeText::default();
            s.push(vm, "[Errno ")?;
            let text = vm.stringify(no)?;
            s.push(vm, &text)?;
            drop(text);
            s.push(vm, "] ")?;
            let text = vm.stringify(msg)?;
            s.push(vm, &text)?;
            drop(text);
            let f = vm.vslot_o(args, 2);
            if n > 2 && f != Value::NONE {
                s.push(vm, ": ")?;
                let text = vm.repr(f)?;
                s.push(vm, &text)?;
                drop(text);
                let f2 = vm.vslot_o(args, 4);
                if n == 5 && f2 != Value::NONE {
                    s.push(vm, " -> ")?;
                    let text = vm.repr(f2)?;
                    s.push(vm, &text)?;
                }
            }
            s
        }
        1 if vm.exc_type(e) == T::KeyError => vm.repr(vm.vslot_o(args, 0))?,
        1 => {
            let x = vm.vslot_o(args, 0);
            if let Some(r) = vm.dunder_call(x, Dn::Str, &[], Cont::Check(check::STR))? {
                return Ok(r);
            }
            vm.stringify(x)?
        }
        _ => vm.repr(args)?,
    };
    vm.new_str(&s)
}

fn exc_with_traceback(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = vm.arg_o(&a, 0);
    vm.set_traceback(e, vm.arg_o(&a, 1))?;
    Ok(e)
}

fn exc_add_note(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (e, n) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !Vm::is_str(n) {
        let msg = format!("note must be a str, not '{}'", vm.type_name(n));
        return Err(vm.err(T::TypeError, &msg));
    }
    let k = vm.intern("__notes__")?;
    let notes = if let Some(l) = vm.inst_get(e, k) {
        l
    } else {
        let l = vm.new_list(&[])?;
        vm.inst_set(e, k, l)?;
        l
    };
    vm.list_push(notes, n)?;
    Ok(Value::NONE)
}

// ------------------------------------------------------- generators

fn gen_send(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let g = vm.arg_o(&a, 0);
    vm.call_request(g, alloc::vec![vm.arg_o(&a, 1)], Cont::GenSend)
}

fn gen_next(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let g = vm.arg_o(&a, 0);
    if Vm::is_native_iter(g) {
        return match vm.iter_next(g)? {
            Some(v) => Ok(v),
            None => Err(vm.raise_stop(Value::NONE)),
        };
    }
    if g.is_obj_kind(ty::CALLITER as u8) {
        if vm.vslot_o(g, 1) == Value::UNDEFINED {
            return Err(vm.raise_stop(Value::NONE));
        }
        return vm.calliter_next(g, u32::MAX);
    }
    vm.call_request(g, alloc::vec![Value::NONE], Cont::GenSend)
}

/// `range_iterator.__length_hint__()`: the items left.
fn range_hint(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let it = vm.check_self(&a, T::range_iterator, "__length_hint__")?;
    let left = it.as_obj().map_or(0, |r| vm.raw_o(r, 3));
    vm.new_int128(i128::from(left))
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn self_iter(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    Ok(vm.arg_o(&a, 0))
}

fn gen_throw(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let g = vm.arg_o(&a, 0);
    let e = thrown(vm, &a)?;
    vm.request = Some(Request::Throw {
        gn: g,
        exc: e,
        cont: Cont::GenSend,
    });
    Ok(Value::UNDEFINED)
}

/// The exception `throw(type[, value[, tb]])` (and `athrow`) raise.
fn thrown(vm: &mut Vm, a: &CallArgs) -> Result<Value, Raised> {
    let (t, v) = (vm.arg_o(a, 1), vm.arg_o(a, 2));
    Ok(
        if v != Value::UNDEFINED && v != Value::NONE && v.is_obj_kind(ty::EXC as u8) {
            v
        } else if v != Value::UNDEFINED && v != Value::NONE && Vm::is_type(t) {
            let ex = vm.raisable(t)?;
            if let Some(r) = ex.as_obj() {
                let args = vm.alloc_tuple(&[v])?;
                vm.set_slot_o(r, crate::vm::exc_slot::ARGS, args);
            }
            ex
        } else if t.is_obj_kind(ty::EXC as u8) || Vm::is_type(t) {
            vm.raisable(t)?
        } else {
            return Err(vm.err(
                T::TypeError,
                "exceptions must be classes or instances deriving from BaseException, not str",
            ));
        },
    )
}

/// `coroutine.send(v)`. A coroutine runs inside `send()` until it returns
/// or an `__await__()` iterator it awaits yields; it is then suspended and
/// `send()` returns the yielded value. An unsettled asyncio awaitable in
/// it suspends the calling task, as awaiting it would.
fn coro_send(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let c = vm.arg_o(&a, 0);
    vm.coro_unstarted(c)?;
    if !c.is_obj_kind(ty::CORO as u8) {
        return Err(vm.err(
            T::RuntimeError,
            "micro-monty: asyncio.sleep() can only be awaited",
        ));
    }
    if vm.coro_state(c) == crate::aio::coro::SUSPENDED {
        vm.coro_deliver(c, vm.arg_o(&a, 1), crate::aio::coro::SEND);
    } else if vm.arg_o(&a, 1) != Value::NONE {
        return Err(vm.err(
            T::TypeError,
            "can't send non-None value to a just-started coroutine",
        ));
    }
    vm.call_request(c, alloc::vec![Value::NONE], Cont::CoroSend)
}

/// `coroutine.throw(exc)`: a coroutine not yet started closes, and the
/// exception propagates.
fn coro_throw(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let c = vm.arg_o(&a, 0);
    vm.coro_unstarted(c)?;
    let e = thrown(vm, &a)?;
    if vm.coro_state(c) == crate::aio::coro::SUSPENDED {
        // Raised where it waits, after the iterator it awaits had its turn.
        vm.coro_deliver(c, e, crate::aio::coro::THROW);
        return vm.call_request(c, alloc::vec![Value::NONE], Cont::CoroSend);
    }
    vm.coro_close(c);
    vm.exc = e;
    Err(Raised)
}

fn coro_close(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let c = vm.arg_o(&a, 0);
    if vm.coro_state(c) == crate::aio::coro::RUNNING {
        return Err(vm.err(T::ValueError, "coroutine already executing"));
    }
    if vm.coro_state(c) == crate::aio::coro::SUSPENDED
        && vm.vslot_o(c, crate::aio::coro::AWAITER) == Value::NONE
    {
        // `GeneratorExit` thrown in; a yield instead is an error.
        return vm.prelude_request("_coro_close", alloc::vec![c], Cont::None);
    }
    vm.coro_close(c);
    Ok(Value::NONE)
}

/// `coroutine.__await__()`: an iterator that drives the coroutine.
fn coro_await(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.prelude_request("_coro_wrapper", alloc::vec![vm.arg_o(&a, 0)], Cont::None)
}

fn agen_anext(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.new_agen_op(vm.arg_o(&a, 0), ag::SEND, Value::NONE)
}

fn agen_asend(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.new_agen_op(vm.arg_o(&a, 0), ag::SEND, vm.arg_o(&a, 1))
}

fn agen_athrow(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = thrown(vm, &a)?;
    vm.new_agen_op(vm.arg_o(&a, 0), ag::THROW, e)
}

fn agen_aclose(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.new_agen_op(vm.arg_o(&a, 0), ag::CLOSE, Value::NONE)
}

fn bi_aiter(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.aiter_of(vm.arg_o(&a, 0), false)
}

/// `anext(it[, default])`; with a default, the prelude's coroutine awaits
/// `it.__anext__()` and catches `StopAsyncIteration`.
fn bi_anext(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (it, d) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !vm.is_async_iterator(it) {
        let msg = format!("'{}' object is not an async iterator", vm.type_name(it));
        return Err(vm.err(T::TypeError, &msg));
    }
    if d != Value::UNDEFINED {
        return vm.prelude_request("_anext_default", alloc::vec![it, d], Cont::None);
    }
    if it.is_obj_kind(ty::AGEN as u8) {
        return vm.new_agen_op(it, ag::SEND, Value::NONE);
    }
    match vm.user_method(it, Dn::Anext) {
        Some(m) => {
            let n = vm.call_dunder(m, it, &[], Cont::None)?;
            vm.next_request(n)
        }
        None => Err(vm.err(T::SystemError, "micro-monty: no __anext__")),
    }
}

fn gen_close(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let g = vm.arg_o(&a, 0);
    if vm.gen_state(g) != crate::generator::g::SUSPENDED {
        if vm.gen_state(g) == crate::generator::g::RUNNING {
            return Err(vm.err(T::ValueError, "generator already executing"));
        }
        vm.gen_done(g);
        return Ok(Value::NONE);
    }
    let e = vm.new_exception(T::GeneratorExit, &[])?;
    vm.request = Some(Request::Throw {
        gn: g,
        exc: e,
        cont: Cont::Close,
    });
    Ok(Value::UNDEFINED)
}

// ---------------------------------------------------------- prelude

fn pl_hash_arg(vm: &mut Vm, v: Value) -> Result<u64, Raised> {
    vm.int_of_o(v)
        .map(i64::cast_unsigned)
        .ok_or_else(|| vm.err(T::TypeError, "hash must be an int"))
}

/// `sys.setrecursionlimit(n)`: CPython's checks; the host's configured
/// limit stays the ceiling (docs/spec.md §11).
fn sys_setrecursionlimit(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.int_arg(vm.arg_o(&a, 0), "")?;
    if n < 1 {
        return Err(vm.err(
            T::ValueError,
            "recursion limit must be greater or equal than 1",
        ));
    }
    let depth = vm.user_frames(vm.frame_base) as i64 + 1;
    if n <= depth {
        let msg = format!(
            "cannot set the recursion limit to {n} at the recursion depth {depth}: the limit is too low"
        );
        return Err(vm.err(T::RecursionError, &msg));
    }
    vm.recursion_limit = u32::try_from(n)
        .unwrap_or(u32::MAX)
        .min(vm.recursion_ceiling);
    Ok(Value::NONE)
}

fn sys_getrecursionlimit(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    vm.new_int(i64::from(vm.recursion_limit))
}

/// `sys.exception()`: the exception being handled, or `None`.
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn sys_exception(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    Ok(vm.handled)
}

/// `sys.exc_info()`: `(type, value, traceback)` of the exception being
/// handled, or three `None`s.
fn sys_exc_info(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    let e = vm.handled;
    if e == Value::NONE {
        return vm.alloc_tuple(&[Value::NONE; 3]);
    }
    let (t, tb) = (vm.class_of(e), vm.traceback_of(e)?);
    vm.alloc_tuple(&[t, e, tb])
}

fn tb_extract(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.tb_extract(vm.arg_o(&a, 0), vm.arg_o(&a, 1))
}

/// `_hint(exc, with_tb)`: the "Did you mean" (or micro-monty note) CPython's
/// `TracebackException` appends to `str(exc)`, or `None`.
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn tb_hint(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = vm.arg_o(&a, 0);
    if !e.is_obj_kind(ty::EXC as u8) {
        return Ok(Value::NONE);
    }
    let with_tb = vm.truthy(vm.arg_o(&a, 1));
    Ok(vm.hint_of(e, with_tb))
}

/// The dict `d` is, or the one a `dict_keys`/`dict_items` view reads (the
/// view twins probe through the view).
fn view_dict(vm: &Vm, d: Value) -> Value {
    match vm.type_of(d) {
        T::dict_keys | T::dict_items => vm.vslot_o(d, 0),
        _ => d,
    }
}

fn pl_dict_probe(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let h = pl_hash_arg(vm, vm.arg_o(&a, 2))?;
    let d = view_dict(vm, vm.arg_o(&a, 0));
    vm.dict_probe_py(d, vm.arg_o(&a, 1), h, vm.arg_o(&a, 3))
}

fn pl_dict_insert(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let h = pl_hash_arg(vm, vm.arg_o(&a, 2))?;
    vm.dict_insert_new(vm.arg_o(&a, 0), vm.arg_o(&a, 1), h, vm.arg_o(&a, 3))?;
    Ok(Value::NONE)
}

fn pl_dict_value_at(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = u32::try_from(vm.int_of_o(vm.arg_o(&a, 1)).unwrap_or(-1)).unwrap_or(u32::MAX);
    let v = vm.dict_value_at(view_dict(vm, vm.arg_o(&a, 0)), e);
    if v == Value::UNDEFINED {
        return Err(vm.err(T::RuntimeError, "dictionary changed size during lookup"));
    }
    Ok(v)
}

/// `_dict_at(d, e)`: entry `e`'s `(key, value)`, False if deleted, None
/// past the last one (a walk that sees later insertions, as `PyDict_Next`).
fn pl_dict_at(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = u32::try_from(vm.int_of_o(vm.arg_o(&a, 1)).unwrap_or(-1)).unwrap_or(u32::MAX);
    match vm.dict_entry_at(view_dict(vm, vm.arg_o(&a, 0)), e) {
        None => Ok(Value::NONE),
        Some((k, _)) if k == Value::UNDEFINED => Ok(Value::FALSE),
        Some((k, v)) => vm.alloc_tuple(&[k, v]),
    }
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn pl_dict_set_at(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = u32::try_from(vm.int_of_o(vm.arg_o(&a, 1)).unwrap_or(-1)).unwrap_or(u32::MAX);
    vm.dict_set_at(vm.arg_o(&a, 0), e, vm.arg_o(&a, 2));
    Ok(Value::NONE)
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn pl_dict_del_at(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = u32::try_from(vm.int_of_o(vm.arg_o(&a, 1)).unwrap_or(-1)).unwrap_or(u32::MAX);
    vm.dict_del_at(vm.arg_o(&a, 0), e);
    Ok(Value::NONE)
}

fn pl_set_probe(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let h = pl_hash_arg(vm, vm.arg_o(&a, 2))?;
    let depth = pl_depth(vm, vm.arg_o(&a, 3));
    vm.set_probe_py(vm.arg_o(&a, 0), vm.arg_o(&a, 1), h, depth, vm.arg_o(&a, 4))
}

/// `_more(src, i, items, None)`: item `i` on of a bulk operation's source
/// (`Vm::bulk_item`) as `(position, item, dict value, stored hash or
/// None)`, or `None` past the end.
fn pl_bulk_at(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let i = u64::try_from(vm.int_of_o(vm.arg_o(&a, 1)).unwrap_or(0)).unwrap_or(0);
    let items = vm.truthy(vm.arg_o(&a, 2));
    let Some((at, x, v, h)) = vm.bulk_item(vm.arg_o(&a, 0), i, items)? else {
        return Ok(Value::NONE);
    };
    let at = vm.new_int(i64::try_from(at).unwrap_or(i64::MAX))?;
    let h = match h {
        Some(h) => vm.new_int(h.cast_signed())?,
        None => Value::NONE,
    };
    vm.alloc_tuple(&[at, x, v, h])
}

/// A prelude twin's nesting argument `d` (a small int it counts up).
fn pl_depth(vm: &Vm, v: Value) -> u32 {
    u32::try_from(vm.int_of_o(v).unwrap_or(0)).unwrap_or(u32::MAX)
}

/// The next native step of an operation the prelude goes on with a step
/// per loop iteration (one entry: each prelude native costs startup).
/// `_more(xs, it)`: appends up to `COLLECT_CHUNK` items of `it` to list
/// `xs`, one fuel unit each; true if `it` may have more. `_more(xs, keys,
/// st)`: the next slice of a long sort ([`Vm::sort_more`]): `None` while
/// there is more, the sorted list, or `False` if the keys need Python to
/// compare. `_more(a, b, i, d)`: a slice of `_ssub` ([`pl_set_scan`]);
/// `_more(src, i, items, None)`: an item of `_bulk` ([`pl_bulk_at`]).
fn pl_more(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if a.n == 4 {
        return if vm.arg_o(&a, 3) == Value::NONE {
            pl_bulk_at(vm, a)
        } else {
            pl_set_scan(vm, a)
        };
    }
    if a.n == 1 {
        // A chunked native's next step (`nstep.rs`).
        return vm.chunk_step(vm.arg_o(&a, 0));
    }
    if a.n == 3 {
        let (xs, keys, st) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1), vm.arg_o(&a, 2));
        return match vm.sort_more(xs, keys, st)? {
            Sorted::Done(v) => Ok(v),
            Sorted::More(_) => Ok(Value::NONE),
            Sorted::Fail(crate::ops::OrdErr::Py) => Ok(Value::from_bool(false)),
            Sorted::Fail(e) => Err(vm.order_error(e, "<")),
        };
    }
    let more = vm.list_more(vm.arg_o(&a, 0), vm.arg_o(&a, 1))?;
    Ok(Value::from_bool(more))
}

/// `_seq_scan(a, b, i, d)` for two lists or two tuples at nesting `d` of a
/// comparison: the first index from `i` whose items one native walk cannot
/// prove `==` (they differ, need a user `__eq__`, or the walk spent its
/// budget, [`crate::dict::walk_budget`]), else the shorter length. The
/// prelude's sequence comparisons loop over it, so long sequences compare
/// in native slices with a safepoint between them.
fn pl_seq_scan(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (x, y) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    let mut i = usize::try_from(vm.int_of_o(vm.arg_o(&a, 2)).unwrap_or(0)).unwrap_or(0);
    let depth = pl_depth(vm, vm.arg_o(&a, 3));
    let budget = crate::dict::walk_budget(depth);
    let mut left = budget;
    // The items in place, as the walk reads them (empty for other types).
    let (xa, xb) = (vm.seq_words(x), vm.seq_words(y));
    while let (Some(&p), Some(&q)) = (xa.get(i), xb.get(i)) {
        if p == q {
            // Identical items cost a visit per block, as in the walk.
            let at = u32::try_from(i).unwrap_or(u32::MAX);
            match crate::walk::pass_same(xa, xb, at, &mut left) {
                Ok(j) => i = j as usize,
                Err(j) => {
                    i = j as usize;
                    break;
                }
            }
            continue;
        }
        let v = |w: u64| Value::from_bits(w).unwrap_or(Value::UNDEFINED);
        if vm.eq_hard(v(p), v(q), depth + 1, &mut left) != Ok(true) {
            break;
        }
        i += 1;
    }
    vm.walked_of(budget, left);
    vm.new_int(i64::try_from(i).unwrap_or(i64::MAX))
}

/// `_dict_scan(a, b, e, d)` for two dicts at nesting `d`: the first entry
/// index from `e` of `a` that one native walk cannot match in `b` (see
/// [`Vm::dict_match_from`]), else past the last entry; `_seq_scan` for
/// dicts.
fn pl_dict_scan(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = u32::try_from(vm.int_of_o(vm.arg_o(&a, 2)).unwrap_or(0)).unwrap_or(u32::MAX);
    let depth = pl_depth(vm, vm.arg_o(&a, 3));
    let budget = crate::dict::walk_budget(depth);
    let mut left = budget;
    let (e, _) = vm.dict_match_from(vm.arg_o(&a, 0), vm.arg_o(&a, 1), e, depth, &mut left);
    vm.walked_of(budget, left);
    vm.new_int(i64::from(e))
}

/// `_more(a, b, i, d)`: from slot `i` of set `a` on, finds each
/// element in set `b` natively, inside a comparison at nesting `d` (`-1`:
/// none): `None` once all are, `False` at one that is not, `(slot,
/// element, hash)` for one only Python code can find, or the slot to go
/// on from once the walk's budget is spent, so `_ssub` takes a slice per
/// loop step.
fn pl_set_scan(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (s, o) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    let mut i = usize::try_from(vm.int_of_o(vm.arg_o(&a, 2)).unwrap_or(0)).unwrap_or(0);
    let depth = u32::try_from(vm.int_of_o(vm.arg_o(&a, 3)).unwrap_or(0)).unwrap_or(0);
    let budget = crate::dict::walk_budget(depth);
    let mut left = budget;
    let out = loop {
        let Some((x, h)) = vm.set_entry_from(s, &mut i) else {
            break Ok(Value::NONE);
        };
        if left == 0 {
            break vm.new_int(i as i64);
        }
        match vm.set_find_at(o, x, h, depth, &mut left) {
            Ok(Some(_)) => i += 1,
            Ok(None) => break Ok(Value::FALSE),
            Err(crate::dict::Hard::Py) => {
                let iv = vm.new_int(i as i64)?;
                let hv = vm.new_int(h.cast_signed())?;
                break vm.alloc_tuple(&[iv, x, hv]);
            }
            Err(crate::dict::Hard::Deep) => {
                break Err(vm.err(
                    T::RecursionError,
                    "maximum recursion depth exceeded in comparison",
                ));
            }
        }
    };
    vm.walked_of(budget, left);
    out
}

/// `_walk_eq(x, y, d)`: `x == y` decided by one native walk at nesting `d`
/// of a comparison a prelude twin took over, `None` if the walk cannot
/// (a user `__eq__`, or the budget ran out); `RecursionError` past the
/// depth limit. The twins compare items with it instead of `==`, so the
/// nesting carries across levels they run in bytecode.
fn pl_walk_eq(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let depth = pl_depth(vm, vm.arg_o(&a, 2));
    let budget = crate::dict::walk_budget(depth);
    let mut left = budget;
    let r = vm.eq_hard(vm.arg_o(&a, 0), vm.arg_o(&a, 1), depth, &mut left);
    vm.walked_of(budget, left);
    match r {
        Ok(b) => Ok(Value::from_bool(b)),
        Err(crate::dict::Hard::Py) => Ok(Value::NONE),
        Err(crate::dict::Hard::Deep) => Err(vm.err(
            T::RecursionError,
            "maximum recursion depth exceeded in comparison",
        )),
    }
}

/// `_walk_repr(x, d, ids)`: `repr(x)` from one native walk at nesting `d`
/// of a `repr` the prelude twin took over (the containers it is inside,
/// its `_REPR_IDS`, show as `...`), with the fuel left as its visit budget;
/// `None` if the budget runs out (the visits charged) or Python code must
/// run.
/// `RecursionError` past the depth limit, as natively.
fn pl_walk_repr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (x, outer) = (vm.arg_o(&a, 0), vm.arg_o(&a, 2));
    let depth = pl_depth(vm, vm.arg_o(&a, 1)) as usize;
    let limit = usize::try_from(vm.heap.memory_limit()).unwrap_or(usize::MAX);
    let mut out = NativeText::default();
    let saved = vm.exc;
    let budget = crate::text::REPR_SLICE;
    match vm.repr_walk(x, &mut out, limit, depth, outer, Some(budget)) {
        Ok(Some(true)) => vm.new_str(&out),
        Ok(Some(false)) => Err(vm.err(T::MemoryError, "")),
        Ok(None) => Ok(Value::NONE),
        Err(e) if vm.slow => {
            let _ = e;
            vm.slow = false;
            vm.exc = saved;
            Ok(Value::NONE)
        }
        Err(e) => Err(e),
    }
}

/// `_walk_ord(x, y, d)`: the order of `x` and `y` from one native walk at
/// nesting `d`: -1, 0 (`==`) or 1; 2 if unordered (NaN, sets neither
/// containing the other: every operator is false); `None` if the walk ran
/// out of budget; `NotImplemented` if Python code must decide (a user
/// method, or an unorderable pair, whose operator raises).
fn pl_walk_ord(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    use core::cmp::Ordering as O;
    let depth = pl_depth(vm, vm.arg_o(&a, 2));
    let budget = crate::dict::walk_budget(depth);
    let mut left = budget;
    let r = vm.order_pure(vm.arg_o(&a, 0), vm.arg_o(&a, 1), depth, &mut left);
    vm.walked_of(budget, left);
    let c = match r {
        Ok(Some(O::Less)) => -1,
        Ok(Some(O::Equal)) => 0,
        Ok(Some(O::Greater)) => 1,
        Ok(None) => 2,
        Err(crate::ops::OrdErr::Py) if left == 0 => return Ok(Value::NONE),
        Err(crate::ops::OrdErr::Deep) => {
            return Err(vm.err(
                T::RecursionError,
                "maximum recursion depth exceeded in comparison",
            ));
        }
        Err(_) => return Ok(Value::NOT_IMPLEMENTED),
    };
    Ok(Value::int(c).unwrap_or(Value::NONE))
}

fn pl_set_insert(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let h = pl_hash_arg(vm, vm.arg_o(&a, 2))?;
    vm.set_add_new(vm.arg_o(&a, 0), vm.arg_o(&a, 1), h)?;
    Ok(stored(vm))
}

/// `_set_hashed_at(s, i)`: `([(key, stored hash), ...], j)` for set `s`'s
/// elements from slot `i`, at most `1 << 14` of them, `j` the slot after
/// the last; `None` past the end. The prelude's `_set_hashed` reads a set
/// with them a slice per call, so it never rehashes its elements (CPython
/// reuses the stored hashes) and never builds all their pairs in a step.
fn pl_set_hashed(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let mut i = usize::try_from(vm.int_of_o(vm.arg_o(&a, 1)).unwrap_or(0)).unwrap_or(0);
    let s = vm.arg_o(&a, 0);
    let mut out = Vec::new();
    while out.len() < 1 << 14 {
        let Some((k, h)) = vm.set_entry_from(s, &mut i) else {
            break;
        };
        i += 1;
        let h = vm.new_int(h.cast_signed())?;
        out.push(vm.alloc_tuple(&[k, h])?);
    }
    if out.is_empty() {
        return Ok(Value::NONE);
    }
    let list = vm.new_list(&out)?;
    let j = vm.new_int(i64::try_from(i).unwrap_or(i64::MAX))?;
    vm.alloc_tuple(&[list, j])
}

/// `_set_table(s, x)`, the prelude twins' edits of set `s`'s table: an
/// int `x` removes the entry in slot `x`, a set `x` gives `s` its table
/// ([`Vm::set_move`]), `x` being `s` settles it ([`Vm::set_settle`]),
/// `(src,)` presizes it for a merge of `src` ([`Vm::set_presize`]), and
/// `None` makes `s` a frozenset and gives it back. One native for the
/// five, as each prelude native is bound at startup.
fn pl_set_table(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (s, x) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if s == x {
        vm.set_settle(s)?;
    } else if x.is_obj_kind(ty::TUPLE as u8) {
        // `(src,)`: presized for a merge of set or dict `src`.
        let src = vm.tuple_items(x).first().copied().unwrap_or(Value::NONE);
        vm.set_presize(s, src)?;
    } else if x == Value::NONE {
        // A set the prelude built becomes the frozenset it returns.
        vm.set_freeze(s);
        return Ok(s);
    } else if let Some(i) = vm.int_of_o(x) {
        vm.set_remove_at(s, usize::try_from(i).unwrap_or(usize::MAX));
    } else {
        vm.set_move(s, x);
    }
    Ok(Value::NONE)
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn pl_set_exc_str(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (e, s) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if let Some(r) = e.as_obj().filter(|_| e.is_obj_kind(ty::EXC as u8)) {
        vm.set_slot_o(r, crate::vm::exc_slot::STR, s);
    }
    Ok(Value::NONE)
}

/// `_hash_tuple(t, i, acc, h)`: a slice of tuple `t`'s hash for `_thash`.
/// `h` a list: `t`'s hash from its items' hashes, all computed in Python
/// (a short tuple). Otherwise `acc` is the hash so far (`None` before the
/// first item) and `h`, unless `None`, item `i`'s hash, computed in
/// Python; it hashes the next items
/// natively, at most [`nstep::STEP_WORK`] units of them: the hash, or
/// `(j, acc)` when item `j` is to be hashed next in Python (its hash
/// needs Python code, or the slice is spent).
fn pl_hash_tuple(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    use crate::dict::{XX_SEED, xx_finish, xx_lane};
    let t = vm.arg_o(&a, 0);
    if !t.is_obj_kind(ty::TUPLE as u8) {
        return Err(vm.bad_step());
    }
    let hs = vm.arg_o(&a, 3);
    if hs.is_obj_kind(ty::LIST as u8) {
        let hs = vm.list_items(hs);
        vm.burn(hs.len() as u64);
        let acc = hs
            .iter()
            .fold(XX_SEED, |acc, &h| xx_lane(acc, vm.int_of_o(h).unwrap_or(0)));
        return vm.new_int(xx_finish(acc, Some(hs.len() as u64)));
    }
    let mut i = usize::try_from(vm.int_of_o(vm.arg_o(&a, 1)).unwrap_or(0)).unwrap_or(0);
    let mut acc = match vm.arg_o(&a, 2) {
        Value::NONE => XX_SEED,
        v => vm.int_of_o(v).unwrap_or(0).cast_unsigned(),
    };
    if let Some(h) = vm.int_of_o(vm.arg_o(&a, 3)) {
        acc = xx_lane(acc, h);
        i += 1;
    }
    let n = vm.tuple_len(t);
    let start = vm.work;
    let mut k = 0u64;
    while i < n {
        if k + vm.work.wrapping_sub(start) >= nstep::STEP_WORK {
            break;
        }
        let x = vm.vslot_o(t, u32::try_from(i).unwrap_or(0));
        k += 1;
        match vm.hash(x) {
            Ok(h) => acc = xx_lane(acc, h),
            Err(Raised) if vm.slow => {
                vm.slow = false;
                break;
            }
            Err(e) => {
                vm.burn(k);
                return Err(e);
            }
        }
        i += 1;
    }
    vm.burn(k);
    if i >= n {
        return vm.new_int(xx_finish(acc, Some(n as u64)));
    }
    let i = vm.new_int(i64::try_from(i).unwrap_or(i64::MAX))?;
    let acc = vm.new_int(acc.cast_signed())?;
    vm.alloc_tuple(&[i, acc])
}

fn pl_list_replace(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let items = vm.list_items(vm.arg_o(&a, 1));
    vm.list_set_items(vm.arg_o(&a, 0), &items)?;
    Ok(Value::NONE)
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn pl_lazy(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    // With a second argument, also any builtin iterator (`_mcall`).
    let v = vm.arg_o(&a, 0);
    let fl = vm.uflags(v);
    let lazy = a.n > 1 && vm.is_iterator(v)
        || matches!(v.obj_kind().map(u16::from), Some(ty::GEN | ty::CALLITER))
        || (vm.user_class(v).is_some()
            && (fl & (crate::class::f::ITER | crate::class::f::NEXT) != 0
                || fl & crate::class::f::GETITEM != 0 && vm.container_backing(v).is_none()));
    Ok(Value::from_bool(lazy))
}

fn pl_native_name(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = vm.arg_o(&a, 0);
    match f.as_native().and_then(native) {
        Some(n) if let Some(m) = n.module() => vm.new_str(&format!("{m}.{}", n.name())),
        n => vm.intern(n.map_or("?", |n| n.name())),
    }
}

fn pl_type_name(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    let n = String::from(vm.type_name(v));
    vm.new_str(&n)
}

fn pl_cmp_error(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (x, y, sym) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1), vm.arg_o(&a, 2));
    let msg = format!(
        "'{}' not supported between instances of '{}' and '{}'",
        vm.str_ref_o(sym),
        vm.type_name(x),
        vm.type_name(y)
    );
    Err(vm.err(T::TypeError, &msg))
}

/// `__import__(name, globals=None, locals=None, fromlist=(), level=0)`:
/// the import statement's machinery (a bundled module's body runs as a
/// call request, never a nested run). A non-empty `fromlist` goes to the
/// prelude's `_import_fromlist`, which imports its submodules through
/// `__import__` again.
fn bi_import(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let name = vm.unbx(vm.arg_o(&a, 0));
    if !Vm::is_str(name) {
        return Err(vm.err(T::TypeError, "module name must be a string"));
    }
    let level = match vm.arg_o(&a, 4) {
        Value::UNDEFINED => 0,
        v => vm.int_arg(v, "")?,
    };
    if level < 0 {
        return Err(vm.err(T::ValueError, "level must be >= 0"));
    }
    if level > 0 {
        // No module of a cell is in a package, so CPython's checks end here.
        if matches!(vm.arg_o(&a, 1), Value::UNDEFINED | Value::NONE) {
            return Err(vm.err(T::KeyError, "'__name__' not in globals"));
        }
        return Err(vm.err(
            T::ImportError,
            "attempted relative import with no known parent package",
        ));
    }
    if vm.str_ref_o(name).is_empty() || vm.str_ref_o(name).starts_with('.') {
        return Err(vm.err(T::ValueError, "Empty module name"));
    }
    let fromlist = vm.arg_o(&a, 3);
    let empty = match fromlist {
        Value::UNDEFINED | Value::NONE => true,
        v if matches!(vm.type_of(v), T::list | T::tuple | T::str) => vm.len_of(v)? == 0,
        _ => false,
    };
    if empty {
        return vm.import_checked(name, true, true);
    }
    let pkg = Value::from_bool(vm.is_package(&String::from(vm.str_ref_o(name))));
    vm.prelude_request(
        "_import_fromlist",
        alloc::vec![name, fromlist, pkg],
        Cont::None,
    )
}

fn bi_hasattr(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (o, n) = (vm.arg_o(&a, 0), vm.unbx(vm.arg_o(&a, 1)));
    if !Vm::is_str(n) {
        let msg = format!(
            "attribute name must be string, not '{}'",
            String::from(vm.type_name(n))
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    // Attribute names compare by identity: intern computed ones.
    let n = vm.intern_value(n)?;
    match vm.getattr(o, n) {
        Ok(Value::UNDEFINED) => {
            // A property getter or `__getattr__` runs first.
            vm.wrap_request(Cont::HasAttr);
            Ok(Value::UNDEFINED)
        }
        Ok(_) => Ok(Value::TRUE),
        Err(Raised) if vm.exc_type(vm.exc) == T::AttributeError => Ok(Value::FALSE),
        Err(e) => Err(e),
    }
}

fn bi_int(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    let base = vm.arg_o(&a, 1);
    if v == Value::UNDEFINED {
        if base != Value::UNDEFINED {
            return Err(vm.err(T::TypeError, "int() missing string argument"));
        }
        return Ok(Value::int(0).unwrap_or(Value::NONE));
    }
    if base != Value::UNDEFINED {
        let b = vm.ssize_arg(base)?;
        if !(b == 0 || (2..=36).contains(&b)) {
            return Err(vm.err(T::ValueError, "int() base must be >= 2 and <= 36, or 0"));
        }
        if !Vm::is_str(v) && !Vm::is_bytes_like(v) {
            return Err(vm.err(
                T::TypeError,
                "int() can't convert non-string with explicit base",
            ));
        }
        return vm.int_from_text(v, u32::try_from(b).unwrap_or(10));
    }
    if let Some(i) = vm.int_of_o(v) {
        return vm.new_int(i);
    }
    if Vm::is_heap_int(v) {
        return Ok(v);
    }
    let mut v = v;
    if vm.user_class(v).is_some() {
        // `__index__` only for a type without `__int__` (`Vm::num_backed`).
        let has_int = vm.num_backed(v);
        for (m, k) in [(Dn::Int, check::INT), (Dn::Index, check::INDEX)] {
            if m == Dn::Index && has_int {
                continue;
            }
            if let Some(r) = vm.dunder_call(v, m, &[], Cont::Check(k))? {
                return Ok(r);
            }
        }
        // A builtin-type subclass converts as its value.
        let b = vm.unbx(v);
        if let Some(i) = vm.int_of_o(b) {
            return vm.new_int(i);
        }
        if Vm::is_heap_int(b) {
            return Ok(b);
        }
        v = b;
    }
    if let Some(f) = v.as_float() {
        if f.is_nan() {
            return Err(vm.err(T::ValueError, "cannot convert float NaN to integer"));
        }
        if f.is_infinite() {
            return Err(vm.err(T::OverflowError, "cannot convert float infinity to integer"));
        }
        return vm.new_big(&crate::bigint::Big::from_f64(f));
    }
    if Vm::is_str(v) || Vm::is_bytes_like(v) {
        return vm.int_from_text(v, 10);
    }
    Err(vm.err(
        T::TypeError,
        &format!(
            "int() argument must be a string, a bytes-like object or a real number, not '{}'",
            vm.type_name(v)
        ),
    ))
}

fn bi_float(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let mut v = vm.arg_o(&a, 0);
    if v == Value::UNDEFINED {
        return Ok(Value::float(0.0));
    }
    if let Some(f) = vm.num_to_f64(v)? {
        return Ok(Value::float(f));
    }
    if vm.user_class(v).is_some() {
        if let Some(r) = vm.dunder_call(v, Dn::Float, &[], Cont::Check(check::FLOAT))? {
            return Ok(r);
        }
        if !vm.num_backed(v)
            && let Some(r) = vm.dunder_call(v, Dn::Index, &[], Cont::Check(check::INDEX_FLOAT))?
        {
            return Ok(r);
        }
        // A builtin-type subclass converts as its value.
        let b = vm.unbx(v);
        if let Some(f) = vm.num_to_f64(b)? {
            return Ok(Value::float(f));
        }
        if Vm::is_str(b) {
            v = b;
        }
    }
    if Vm::is_str(v) || Vm::is_bytes_like(v) {
        return vm.num_from_text(v, false);
    }
    Err(vm.err(
        T::TypeError,
        &format!(
            "float() argument must be a string or a real number, not '{}'",
            vm.type_name(v)
        ),
    ))
}

fn bi_str(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg(&a, 0);
    if v == Value::UNDEFINED {
        return vm.intern("");
    }
    let (enc, errs) = (vm.arg(&a, 1), vm.arg(&a, 2));
    if enc != Value::UNDEFINED || errs != Value::UNDEFINED {
        for (x, name) in [(enc, "encoding"), (errs, "errors")] {
            if x != Value::UNDEFINED && !Vm::is_str(x) {
                let msg = format!(
                    "str() argument '{name}' must be str, not {}",
                    vm.type_name(x)
                );
                return Err(vm.err(T::TypeError, &msg));
            }
        }
        if Vm::is_str(v) {
            return Err(vm.err(T::TypeError, "decoding str is not supported"));
        }
        if !Vm::is_bytes_like(v) {
            let msg = format!(
                "decoding to str: need a bytes-like object, {} found",
                vm.type_name(v)
            );
            return Err(vm.err(T::TypeError, &msg));
        }
        return vm.decode_bytes(v, enc, errs);
    }
    if Vm::is_str(v) {
        return Ok(v);
    }
    if let Some(i) = v.as_int() {
        return vm.int_text(i);
    }
    if let Some(b) = vm.str_backing(v) {
        return Ok(b);
    }
    let fl = vm.uflags(v);
    if fl & crate::class::f::STR != 0 {
        if let Some(r) = vm.dunder_call(v, Dn::Str, &[], Cont::Check(check::STR))? {
            return Ok(r);
        }
    } else if fl & crate::class::f::REPR != 0
        && !v.is_obj_kind(ty::EXC as u8)
        && let Some(r) = vm.dunder_call(v, Dn::Repr, &[], Cont::Check(check::REPR))?
    {
        return Ok(r);
    }
    let s = vm.stringify(v)?;
    vm.new_str(&s)
}

#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
fn bi_bool(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if v != Value::UNDEFINED
        && let Some(r) = vm.op_truth(v, crate::cont::truth::BOOL, 0)?
    {
        return Ok(r);
    }
    Ok(Value::from_bool(v != Value::UNDEFINED && vm.truthy(v)))
}

/// A copy of long list or tuple `v` as a list or a tuple, a slice per
/// step; `None` for anything else.
fn seq_copy_long(vm: &mut Vm, v: Value, tuple: bool) -> Result<Option<Value>, Raised> {
    if !(v.is_obj_kind(ty::LIST as u8) || v.is_obj_kind(ty::TUPLE as u8)) {
        return Ok(None);
    }
    let n = vm.seq_words(v).len();
    if n < crate::ops::SEQ_LONG {
        return Ok(None);
    }
    // Charged by the steps that copy it, as a slice copy is (a unit per
    // 8 items), not up front, so no fuel slice is overdrawn.
    let segs = [crate::ops::Seg::of(3, 0, 1, n as u64)];
    vm.seq_new(tuple, &[v], &segs, n).map(Some)
}

fn bi_list(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if v == Value::UNDEFINED {
        return vm.new_list(&[]);
    }
    if let Some(l) = seq_copy_long(vm, v, false)? {
        return Ok(l);
    }
    if let Some(l) = vm.range_seq(v, false)? {
        return Ok(l);
    }
    let items = vm.collect(v)?;
    vm.new_list(&items)
}

fn bi_tuple(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if v == Value::UNDEFINED {
        return vm.alloc_tuple(&[]);
    }
    if v.is_obj_kind(ty::TUPLE as u8) {
        return Ok(v);
    }
    if let Some(t) = seq_copy_long(vm, v, true)? {
        return Ok(t);
    }
    if let Some(t) = vm.range_seq(v, true)? {
        return Ok(t);
    }
    let items = vm.collect(v)?;
    vm.alloc_tuple(&items)
}

/// The bulk phase that fills dict `out` (register 0) from `update()`
/// source `src` (register `reg`): a dict's entries as they are, any other
/// iterable's pairs, read as a list. A user mapping's `keys()` decides
/// in the prelude twin.
pub(crate) fn dict_fill_phase(vm: &mut Vm, src: Value, reg: u64) -> Result<(Value, u64), Raised> {
    use crate::set::{act, phase};
    let src = match vm.backing(src) {
        Some(b) if b.is_obj_kind(ty::DICT as u8) && vm.uflags(src) & crate::class::f::ITER == 0 => {
            b
        }
        // A dict's items view: its pairs, as the dict's entries.
        _ if vm.type_of(src) == T::dict_items && vm.vslot_o(src, 0).is_obj_kind(ty::DICT as u8) => {
            vm.vslot_o(src, 0)
        }
        _ => src,
    };
    if src.is_obj_kind(ty::DICT as u8) {
        return Ok((src, phase(act::DICT, 0, reg, 0)));
    }
    if vm.user_class(src).is_some() && vm.backing(src).is_none() {
        return Err(vm.need_py());
    }
    Ok((vm.bulk_src(src)?, phase(act::PAIRS, 0, reg, 0)))
}

/// `dict(src, **kw)` into `d` (value `res`), in steps.
fn dict_build(vm: &mut Vm, a: &CallArgs, d: Value, res: u64) -> Result<Value, Raised> {
    let mut regs = [d, Value::NONE, Value::NONE];
    let mut prog = 0;
    let mut k = 0;
    for (src, reg) in [
        (vm.arg_o(a, 1 - u32::from(res == 0)), 1),
        (vm.kwarg(a, 0), 2),
    ] {
        if src != Value::UNDEFINED {
            let (src, p) = dict_fill_phase(vm, src, reg)?;
            regs[reg as usize] = src;
            prog |= p << (16 * k);
            k += 1;
        }
    }
    vm.bulk(&regs, prog, res, true)
}

fn bi_dict(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let d = vm.new_dict()?;
    dict_build(vm, &a, d, 0)
}

/// `set(src)` (a frozenset with `frozen`), in steps.
fn set_build(vm: &mut Vm, src: Value, frozen: bool) -> Result<Value, Raised> {
    use crate::set::{act, phase};
    let s = vm.new_set()?;
    if frozen {
        vm.set_freeze(s);
    }
    if src == Value::UNDEFINED {
        return Ok(s);
    }
    let (src, items) = vm.view_src(src)?;
    vm.bulk(&[s, src], phase(act::ADD | items, 0, 1, 0), 0, true)
}

fn bi_set(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    set_build(vm, vm.arg_o(&a, 0), false)
}

fn bi_frozenset(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let src = vm.arg_o(&a, 0);
    if vm.type_of(src) == T::frozenset {
        return Ok(src);
    }
    set_build(vm, src, true)
}

fn bi_range(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let mut nums = [0i64; 3];
    for i in 0..a.n {
        nums[i as usize] = vm.int_arg(vm.arg_o(&a, i), "range")?;
    }
    let (start, stop, step) = match a.n {
        1 => (0, nums[0], 1),
        2 => (nums[0], nums[1], 1),
        _ => (nums[0], nums[1], nums[2]),
    };
    if step == 0 {
        return Err(vm.err(T::ValueError, "range() arg 3 must not be zero"));
    }
    vm.new_range(start, stop, step)
}

fn bi_type(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let kw = vm.kwarg(&a, 0);
    let kw = if kw == Value::UNDEFINED || vm.dict_len(kw) == 0 {
        Value::NONE
    } else {
        kw
    };
    if a.n == 1 && kw != Value::NONE {
        return Err(vm.err(T::TypeError, "type() takes no keyword arguments"));
    }
    match a.n {
        1 => Ok(vm.class_of(vm.arg_o(&a, 0))),
        3 => {
            let (name, bases, ns) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1), vm.arg_o(&a, 2));
            // A base's metaclass other than `type` (ABCMeta, EnumType, ...)
            // builds the class, as CPython's winning metaclass does.
            if bases.is_obj_kind(ty::TUPLE as u8)
                && let Some(m) = vm
                    .tuple_items(bases)
                    .into_iter()
                    .find_map(|b| vm.meta_of(b))
            {
                vm.request = Some(Request::Call {
                    func: m,
                    args: alloc::vec![name, bases, ns].into(),
                    kw,
                    cont: Cont::None,
                    outer: Cont::None,
                });
                return Ok(Value::UNDEFINED);
            }
            vm.type_new(name, bases, ns, kw, Value::NONE)
        }
        _ => Err(vm.err(T::TypeError, "type() takes 1 or 3 arguments")),
    }
}

fn bi_enumerate(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    // The start before the iterable, as CPython's `PyNumber_Index`: an int
    // subclass counts as its int, whatever its `__index__`; another type
    // with `__index__` goes to the twin, which reads it there.
    // The count: an `i64` in raw word 2 while it fits, else an int in
    // value slot 1 with raw word 2 at `i64::MAX` (`Vm::enum_big`).
    let mut start = vm.arg_o(&a, 1);
    if vm.int_of_o(start).is_none() && vm.backing(start).is_some_and(Vm::is_intlike) {
        start = vm.unbx(start);
    }
    let (n, big) = if start == Value::UNDEFINED {
        (0, Value::NONE)
    } else if Vm::is_heap_int(start) && vm.int_of_o(start).is_none() {
        (i64::MAX, start)
    } else {
        (vm.int_arg(start, "start")?, Value::NONE)
    };
    let it = vm.get_iter(vm.arg_o(&a, 0))?;
    if !Vm::is_native_iter(it) {
        return Err(vm.need_py());
    }
    let r = {
        let res = vm
            .heap
            .alloc_init(ty::ENUMERATE, 0, &[it, big], &[n.cast_unsigned()]);
        vm.hx_o(res)
    }?;
    Ok(Value::from_ref(r, ty::ENUMERATE as u8))
}

fn bi_zip(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let strict = vm.kwarg(&a, 0);
    if strict != Value::UNDEFINED && vm.truthy(strict) {
        return Err(vm.need_py());
    }
    let mut iters = Vec::with_capacity(a.n as usize);
    for i in 0..a.n {
        let it = vm.get_iter(vm.arg_o(&a, i))?;
        if !Vm::is_native_iter(it) {
            return Err(vm.need_py());
        }
        iters.push(it);
    }
    let t = vm.alloc_tuple(&iters)?;
    let r = {
        let res = vm.heap.alloc_init(ty::ZIP, 0, &[t], &[]);
        vm.hx_o(res)
    }?;
    Ok(Value::from_ref(r, ty::ZIP as u8))
}

fn bi_slice(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let get = |vm: &Vm, i| {
        let v = vm.arg_o(&a, i);
        if v == Value::UNDEFINED {
            Value::NONE
        } else {
            v
        }
    };
    let parts = if a.n == 1 {
        [Value::NONE, get(vm, 0), Value::NONE]
    } else {
        [get(vm, 0), get(vm, 1), get(vm, 2)]
    };
    let r = {
        let res = vm.heap.alloc_init(ty::SLICE, 0, &parts, &[]);
        vm.hx_o(res)
    }?;
    Ok(Value::from_ref(r, ty::SLICE as u8))
}

fn bi_object(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    let o = vm.type_obj(T::object);
    vm.new_instance(o, 0)
}

// ------------------------------------------------------------- list

/// Items from which a full list grows a slice per step
/// ([`list_grow_steps`]): one copy of more takes milliseconds.
const GROW_LONG: usize = 1 << 20;

fn list_append(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let l = vm.check_self(&a, T::list, "append")?;
    let x = vm.arg(&a, 1);
    match vm.list_try_push(l, x)? {
        Some((_, len)) if len >= GROW_LONG as u64 => {
            let len = len as usize;
            list_grow_steps(vm, l, len, len, x)
        }
        Some((buf, len)) => vm.list_grow(l, buf, len, x).map(|()| Value::NONE),
        None => Ok(Value::NONE),
    }
}

/// Full list `l` of `len` items with `x` inserted at `k`: a new buffer,
/// grown as a push grows it, of the head, `x` and the tail, a slice per
/// step (8M items copied to a fresh buffer took 35 ms in one [M]).
#[cold]
#[inline(never)]
fn list_grow_steps(vm: &mut Vm, l: Value, len: usize, k: usize, x: Value) -> Result<Value, Raised> {
    use crate::ops::Seg;
    let one = vm.alloc_tuple(&[x])?;
    let segs = [
        Seg::of(3, 0, 1, k as u64),
        Seg::of(4, 0, 1, 1),
        Seg::of(3, k as i64, 1, (len - k) as u64),
    ];
    let max = (mm_core::MAX_OBJECT_BYTES as usize / 8).max(len + 1);
    let buf = vm.alloc_items((len * 2).clamp(4, max))?;
    let buf = Value::from_ref(buf, ty::BUF as u8);
    vm.seq_build(l, Value::NONE, buf, &[l, one], &segs, len + 1, 0, 1)
}

fn list_extend(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let l = vm.check_self(&a, T::list, "extend")?;
    let src = vm.arg_o(&a, 1);
    if let Some(r) = vm.list_extend_long(l, src) {
        return r;
    }
    // `l.extend(l)` reads the list's items, as CPython does, not a list
    // subclass's own `__iter__`.
    if vm.unbx(src) == l {
        let items = vm.list_items(l);
        vm.list_extend_values(l, &items)?;
        return Ok(Value::NONE);
    }
    vm.list_extend_from(l, src)?;
    Ok(Value::NONE)
}

fn list_pop(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let l = vm.check_self(&a, T::list, "pop")?;
    let idx = vm.arg_o(&a, 1);
    let i = if idx == Value::UNDEFINED {
        -1
    } else {
        vm.int_arg(idx, "pop")?
    };
    let n = vm.list_len(l);
    if n == 0 {
        return Err(vm.err(T::IndexError, "pop from empty list"));
    }
    let Some(k) = Vm::norm_index(i, n) else {
        return Err(vm.err(T::IndexError, "pop index out of range"));
    };
    let v = vm.list_get(l, k);
    if k + 1 == n {
        if let Some(r) = l.as_obj() {
            vm.set_raw_o(r, 1, (n - 1) as u64);
            if let Some(b) = vm.list_buf(l) {
                vm.set_slot_o(b, u32::try_from(n - 1).unwrap_or(0), Value::UNDEFINED);
            }
        }
        return Ok(v);
    }
    vm.list_del_at(l, k, v)
}

fn list_insert(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    use crate::ops::SEQ_LONG;
    let l = vm.check_self(&a, T::list, "insert")?;
    let i = vm.int_arg(vm.arg_o(&a, 1), "insert")?;
    let x = vm.arg_o(&a, 2);
    let len = vm.list_len(l);
    let n = i64::try_from(len).unwrap_or(0);
    let k = usize::try_from(if i < 0 { (i + n).max(0) } else { i.min(n) }).unwrap_or(0);
    let full = vm
        .list_parts(l)
        .is_none_or(|(buf, _)| u64::from(vm.obj_len(buf)) <= len as u64);
    if full && (len - k >= SEQ_LONG || len >= GROW_LONG) {
        return list_grow_steps(vm, l, len, k, x);
    }
    // In place, as CPython's `memmove`: appending grows the buffer as a
    // push does, then the tail moves up a slot (a unit per 8 items).
    vm.burn((len - k) as u64 / 8);
    vm.list_push(l, x)?;
    if let Some((buf, _)) = vm.list_parts(l) {
        let at = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
        let res = vm.heap.move_values(buf, at(k), at(k + 1), at(len - k));
        vm.hx_o(res)?;
        let res = vm.heap.set_values(buf, at(k), &[x]);
        vm.hx_o(res)?;
    }
    Ok(Value::NONE)
}

fn list_remove(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let l = vm.check_self(&a, T::list, "remove")?;
    vm.seq_scan(l, vm.arg_o(&a, 1), 2, 0, usize::MAX)
}

fn seq_index(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.arg_o(&a, 0);
    let n = vm.seq_words(s).len();
    let bound = |vm: &mut Vm, v: Value, d: usize| -> Result<usize, Raised> {
        if v == Value::UNDEFINED {
            return Ok(d);
        }
        if vm.int_of_o(vm.unbx(v)).is_none()
            && !Vm::is_heap_int(v)
            && vm.uflags(v) & crate::class::f::INDEX == 0
        {
            let msg = "slice indices must be integers or have an __index__ method";
            return Err(vm.err(T::TypeError, msg));
        }
        let i = crate::bytes::search_bound(vm, v, 0)?;
        let ni = i64::try_from(n).unwrap_or(0);
        Ok(usize::try_from(if i < 0 { (i + ni).max(0) } else { i.min(ni) }).unwrap_or(0))
    };
    let start = bound(vm, vm.arg_o(&a, 2), 0)?;
    let stop = bound(vm, vm.arg_o(&a, 3), n)?;
    vm.seq_scan(s, vm.arg_o(&a, 1), 1, start, stop)
}

fn seq_count(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    vm.seq_scan(vm.arg_o(&a, 0), vm.arg_o(&a, 1), 0, 0, usize::MAX)
}

fn range_index(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let r = vm.check_self(&a, T::range, "index")?;
    range_search(vm, r, vm.arg_o(&a, 1), 1)
}

fn range_count(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let r = vm.check_self(&a, T::range, "count")?;
    range_search(vm, r, vm.arg_o(&a, 1), 0)
}

/// `range.count(x)` (`kind` 0) and `range.index(x)` (1), as CPython's: in
/// closed form for an `int` or `bool`; any other `x` compares item by item
/// in the prelude, a unit per item.
#[cfg_attr(mm_shipped, optimize(size))]
fn range_search(vm: &mut Vm, r: Value, x: Value, kind: i64) -> Result<Value, Raised> {
    let pos = match vm.int_of_o(x) {
        Some(i) => {
            let (s, e, st) = vm.range_parts(r);
            let inside = if st > 0 {
                s <= i && i < e
            } else {
                e < i && i <= s
            };
            let d = i128::from(i) - i128::from(s);
            (inside && d % i128::from(st) == 0).then(|| d / i128::from(st))
        }
        // An int past `i64` is past every item.
        None if Vm::is_heap_int(x) => None,
        None => {
            let n = i128::from(vm.range_len(r));
            let mut args = alloc::vec![r, x];
            for w in [0, n, i128::from(kind), 0] {
                args.push(vm.int_from_i128(w)?);
            }
            let f = vm.prelude_fn("_seq_scan_rest")?;
            return Ok(vm.request_call(f, args, Cont::None));
        }
    };
    match (kind, pos) {
        (0, _) => vm.new_int(i64::from(pos.is_some())),
        (_, Some(p)) => vm.int_from_i128(p),
        _ => Err(vm.err(T::ValueError, "range.index(x): x not in range")),
    }
}

fn list_sort(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let l = vm.check_self(&a, T::list, "sort")?;
    let key = vm.kwarg(&a, 0);
    let rev = vm.kwarg(&a, 1);
    let rev = rev != Value::UNDEFINED && vm.truthy(rev);
    if key != Value::UNDEFINED && key != Value::NONE || sort_is_long(vm.list_len(l)) {
        // CPython detaches the items while it sorts: key calls (and, past
        // a park, anything) see `[]`. The buffer moves, not the items.
        let xs = vm.new_list(&[])?;
        vm.list_move(xs, l)?;
        let key = if key == Value::UNDEFINED {
            Value::NONE
        } else {
            key
        };
        return vm.sort_start(xs, key, rev, l);
    }
    let mut items = vm.collect(l)?;
    sort_values(vm, &mut items, rev, true)?;
    // Sorting cannot change the length; reuse the list's admitted backing store.
    if let Some(buf) = vm.list_buf(l) {
        let res = vm.heap.set_values(buf, 0, &items);
        vm.hx_o(res)?;
    }
    Ok(Value::NONE)
}

fn list_reverse(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let l = vm.check_self(&a, T::list, "reverse")?;
    let n = vm.list_len(l);
    if n >= crate::ops::SEQ_LONG {
        let segs = [crate::ops::Seg::of(3, n as i64 - 1, -1, n as u64)];
        return vm.list_rebuild(l, Value::NONE, &[l], &segs, n);
    }
    let mut items = vm.list_items(l);
    items.reverse();
    vm.list_set_items(l, &items)?;
    Ok(Value::NONE)
}

fn list_copy(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let l = vm.check_self(&a, T::list, "copy")?;
    if let Some(v) = seq_copy_long(vm, l, false)? {
        return Ok(v);
    }
    let items = vm.list_items(l);
    vm.new_list(&items)
}

fn list_clear(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let l = vm.check_self(&a, T::list, "clear")?;
    vm.list_set_items(l, &[])?;
    Ok(Value::NONE)
}

// ------------------------------------------------------------- dict

fn dict_get(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let d = vm.check_self(&a, T::dict, "get")?;
    let dflt = vm.arg_o(&a, 2);
    match vm.dict_get(d, vm.arg_o(&a, 1))? {
        Some(v) => Ok(v),
        None => Ok(if dflt == Value::UNDEFINED {
            Value::NONE
        } else {
            dflt
        }),
    }
}

fn dict_pop(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let d = vm.check_self(&a, T::dict, "pop")?;
    let k = vm.arg_o(&a, 1);
    if let Some(v) = vm.dict_del(d, k)? {
        Ok(v)
    } else {
        let dflt = vm.arg_o(&a, 2);
        if dflt != Value::UNDEFINED {
            return Ok(dflt);
        }
        Err(vm.err_value(T::KeyError, k))
    }
}

/// A dict view's kind word (0 keys, 1 values, 2 items) with this bit: made
/// through a dict subclass's instance, whose keys view does not presize a
/// set as its dict (`dictviews_to_set` presizes for an exact dict only).
pub(crate) const VIEW_SUB: u64 = 4;

fn view(vm: &mut Vm, a: &CallArgs, kind: u64, name: &str) -> Result<Value, Raised> {
    // `keys` gets a subclass's instance as itself (`keeps_instance`).
    let s = vm.arg_o(a, 0);
    let d = vm.unbx(s);
    if vm.type_of(d) != T::dict {
        vm.check_self(a, T::dict, name)?;
    }
    let kind = if s == d { kind } else { kind | VIEW_SUB };
    let r = {
        let res = vm.heap.alloc_init(ty::VIEW, 0, &[d], &[kind]);
        vm.hx_o(res)
    }?;
    Ok(Value::from_ref(r, ty::VIEW as u8))
}

fn dict_keys(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    view(vm, &a, 0, "keys")
}

fn dict_values(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    view(vm, &a, 1, "values")
}

fn dict_items(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    view(vm, &a, 2, "items")
}

fn dict_setdefault(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let d = vm.check_self(&a, T::dict, "setdefault")?;
    let k = vm.arg_o(&a, 1);
    if let Some(v) = vm.dict_get(d, k)? {
        return Ok(v);
    }
    let dflt = vm.arg_o(&a, 2);
    let v = if dflt == Value::UNDEFINED {
        Value::NONE
    } else {
        dflt
    };
    // A big table grows in steps; the request's value is `v` (register 2).
    // `k` hashed natively in `dict_get`, so hashing it again runs no
    // Python code.
    let h = vm.key_hash(k)?;
    vm.dict_setdefault_new(d, k, h, v)?;
    Ok(if vm.request.is_some() {
        Value::UNDEFINED
    } else {
        v
    })
}

fn dict_update(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let d = vm.check_self(&a, T::dict, "update")?;
    dict_build(vm, &a, d, crate::set::res::NONE)
}

fn dict_copy(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    use crate::set::{act, phase};
    let d = vm.check_self(&a, T::dict, "copy")?;
    let out = vm.new_dict()?;
    vm.bulk(&[out, d], phase(act::DICT, 0, 1, 0), 0, true)
}

fn dict_clear(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let d = vm.check_self(&a, T::dict, "clear")?;
    vm.dict_clear(d)?;
    Ok(Value::NONE)
}

fn dict_popitem(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let d = vm.check_self(&a, T::dict, "popitem")?;
    let Some((k, v)) = vm.dict_pop_last(d) else {
        return Err(vm.err(T::KeyError, "popitem(): dictionary is empty"));
    };
    vm.alloc_tuple(&[k, v])
}

// -------------------------------------------------------------- set

fn set_add(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.check_self(&a, T::set, "add")?;
    vm.set_add(s, vm.arg_o(&a, 1))?;
    Ok(stored(vm))
}

fn set_remove(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.check_self(&a, T::set, "remove")?;
    let k = vm.arg_o(&a, 1);
    if vm.set_discard(s, k)? {
        Ok(Value::NONE)
    } else {
        Err(vm.err_value(T::KeyError, k))
    }
}

fn set_discard(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.check_self(&a, T::set, "discard")?;
    vm.set_discard(s, vm.arg_o(&a, 1))?;
    Ok(Value::NONE)
}

fn set_pop(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.check_self(&a, T::set, "pop")?;
    vm.set_pop(s)
}

/// `s <op> o` for a set method's argument `o`, any iterable: a set as
/// the operator does it, a dict by its keys, anything else read as a list
/// (CPython's `set_intersection`, `set_difference`, ...); in steps with
/// `top`.
fn set_with(
    vm: &mut Vm,
    op: mm_compile::ast::BinOp,
    s: Value,
    o: Value,
    top: bool,
) -> Result<Value, Raised> {
    use crate::set::{act, phase};
    use mm_compile::ast::BinOp;
    // A set subclass's instance is read as its set (`PyAnySet_Check`).
    let o = if vm.backing(o).is_some_and(Vm::is_set) {
        vm.unbx(o)
    } else {
        o
    };
    if Vm::is_set(o) {
        return vm.set_binop(op, s, o, false, top);
    }
    if op == BinOp::BitAnd {
        let (src, items) = early_src(vm, o)?;
        let out = vm.new_set_like(s)?;
        return vm.bulk(&[out, src, s], phase(act::AND | items, 0, 1, 2), 0, top);
    }
    let src = vm.bulk_src(o)?;
    let out = vm.new_set_like(s)?;
    let dict = src.is_obj_kind(ty::DICT as u8);
    let prog = match op {
        BinOp::Sub if dict && vm.set_len(s) >> 2 <= vm.dict_len(src) => phase(act::SUB, 0, 2, 1),
        BinOp::Sub => phase(act::ADD, 0, 2, 0) | phase(act::DISCARD, 0, 1, 0) << 16,
        // CPython's `set_symmetric_difference`: the set of `o`'s items
        // (its duplicates once), then each element of `s` toggled in.
        BinOp::BitXor => phase(act::ADD, 0, 1, 0) | phase(act::TOGGLE, 0, 2, 0) << 16,
        _ => phase(act::ADD, 0, 2, 0) | phase(act::ADD, 0, 1, 0) << 16,
    };
    vm.bulk(&[out, src, s], prog, 0, top)
}

/// The source of an intersection or subset test with `o`, which CPython
/// stops reading once the answer is known: read in place (a dict's keys
/// or items view through its dict), or else the prelude twin iterates `o`
/// lazily, as CPython does, so a generator is read no further.
fn early_src(vm: &mut Vm, o: Value) -> Result<(Value, u64), Raised> {
    let in_place = matches!(
        o.obj_kind().map(u16::from),
        Some(ty::LIST | ty::TUPLE | ty::SET | ty::DICT | ty::RANGE)
    ) || matches!(vm.type_of(o), T::dict_keys | T::dict_items);
    if !in_place {
        return Err(vm.need_py());
    }
    vm.view_src(o)
}

/// Every `others` of an intersection read in place, or the whole call
/// goes to the prelude twin before any of them is read.
fn early_srcs(vm: &mut Vm, a: &CallArgs) -> Result<(), Raised> {
    for i in 2..a.n {
        early_src(vm, vm.arg_o(a, i))?;
    }
    Ok(())
}

/// A set method over `*others`: the first in steps when it is the only
/// one, the others (rare) each at once.
fn set_many(
    vm: &mut Vm,
    a: &CallArgs,
    op: mm_compile::ast::BinOp,
    name: &str,
) -> Result<Value, Raised> {
    use crate::set::{act, phase, res};
    use mm_compile::ast::BinOp;
    let mut s = vm.check_anyset(a, name)?;
    let first = s;
    if a.n == 1 {
        // A copy, in steps.
        let out = vm.new_set_like(s)?;
        return vm.bulk(&[out, s], phase(act::ADD, 0, 1, 0), 0, true);
    }
    if op == BinOp::BitAnd {
        early_srcs(vm, a)?;
    }
    s = set_with(vm, op, s, vm.arg_o(a, 1), a.n == 2)?;
    for i in 2..a.n {
        if op == BinOp::BitAnd {
            s = set_with(vm, op, s, vm.arg_o(a, i), false)?;
            continue;
        }
        // CPython's `set_union` and `set_difference_multi`: the result of
        // the first updated in place by the others, not copied again; a
        // union skips the set itself.
        if op == BinOp::BitOr && vm.arg_o(a, i) == first {
            continue;
        }
        let (src, items) = vm.view_src(vm.arg_o(a, i))?;
        let act = if op == BinOp::Sub {
            act::DISCARD
        } else {
            act::ADD
        };
        vm.bulk(&[s, src], phase(act | items, 0, 1, 0), res::NONE, false)?;
    }
    Ok(s)
}

fn set_union(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    set_many(vm, &a, mm_compile::ast::BinOp::BitOr, "union")
}

fn set_intersection(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    set_many(vm, &a, mm_compile::ast::BinOp::BitAnd, "intersection")
}

fn set_difference(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    set_many(vm, &a, mm_compile::ast::BinOp::Sub, "difference")
}

fn set_symmetric_difference(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.check_anyset(&a, "symmetric_difference")?;
    set_with(vm, mm_compile::ast::BinOp::BitXor, s, vm.arg_o(&a, 1), true)
}

/// `s.<op>_update(o)` in place (CPython's `set_ior` and friends; any
/// iterable `o` through [`set_with`] for `&` and `^`).
fn set_inplace(
    vm: &mut Vm,
    op: mm_compile::ast::BinOp,
    s: Value,
    o: Value,
    top: bool,
) -> Result<Value, Raised> {
    use crate::set::{act, phase, res};
    use mm_compile::ast::BinOp;
    // A set subclass's instance is read as its set (`PyAnySet_Check`).
    let o = if vm.backing(o).is_some_and(Vm::is_set) {
        vm.unbx(o)
    } else {
        o
    };
    if Vm::is_set(o) {
        // A request (the operation goes on in steps) or the set.
        let v = vm.set_binop(op, s, o, true, top)?;
        return Ok(if v == Value::UNDEFINED {
            v
        } else {
            Value::NONE
        });
    }
    if op == BinOp::BitAnd {
        let (src, items) = early_src(vm, o)?;
        let tmp = vm.new_set()?;
        let p = phase(act::AND | items, 2, 1, 0) | phase(act::MOVE, 0, 2, 0) << 16;
        return vm.bulk(&[s, src, tmp], p, res::NONE, top);
    }
    let src = vm.bulk_src(o)?;
    let dict = src.is_obj_kind(ty::DICT as u8);
    let (regs, prog): (&[Value], u64) = match op {
        BinOp::BitXor if !dict => {
            let tmp = vm.new_set()?;
            let p = phase(act::ADD, 2, 1, 0) | phase(act::TOGGLE, 0, 2, 0) << 16;
            return vm.bulk(&[s, src, tmp], p, res::NONE, top);
        }
        BinOp::BitXor => (&[s, src], phase(act::TOGGLE, 0, 1, 0)),
        BinOp::Sub => (&[s, src], phase(act::DISCARD, 0, 1, 0)),
        _ => (&[s, src], phase(act::ADD, 0, 1, 0)),
    };
    vm.bulk(regs, prog, res::NONE, top)
}

fn set_update_many(
    vm: &mut Vm,
    a: &CallArgs,
    op: mm_compile::ast::BinOp,
    name: &str,
) -> Result<Value, Raised> {
    let s = vm.check_self(a, T::set, name)?;
    if a.n == 2 {
        return set_inplace(vm, op, s, vm.arg_o(a, 1), true);
    }
    if op == mm_compile::ast::BinOp::BitAnd {
        early_srcs(vm, a)?;
    }
    for i in 1..a.n {
        set_inplace(vm, op, s, vm.arg_o(a, i), false)?;
    }
    Ok(Value::NONE)
}

fn set_symmetric_difference_update(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    set_update_many(
        vm,
        &a,
        mm_compile::ast::BinOp::BitXor,
        "symmetric_difference_update",
    )
}

fn set_intersection_update(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    set_update_many(
        vm,
        &a,
        mm_compile::ast::BinOp::BitAnd,
        "intersection_update",
    )
}

fn set_difference_update(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    set_update_many(vm, &a, mm_compile::ast::BinOp::Sub, "difference_update")
}

fn set_isdisjoint(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    use crate::set::{act, phase, res};
    let s = vm.check_anyset(&a, "isdisjoint")?;
    let o = vm.arg_o(&a, 1);
    if o == s {
        return Ok(Value::from_bool(vm.set_len(s) == 0));
    }
    // CPython's `set_isdisjoint`: the smaller set's elements probe the
    // larger; any other iterable's items probe `s`.
    // Anything but a set is read in place or by the prelude twin, which
    // stops at the first common item.
    let (src, other, items) = if Vm::is_set(o) && vm.set_len(o) > vm.set_len(s) {
        (s, o, 0)
    } else if Vm::is_set(o) {
        (o, s, 0)
    } else {
        let (src, items) = early_src(vm, o)?;
        (src, s, items)
    };
    vm.bulk(
        &[src, other],
        phase(act::DISJOINT | items, 0, 0, 1),
        res::TRUE,
        true,
    )
}

/// `dict_keys.isdisjoint` / `dict_items.isdisjoint`.
fn view_isdisjoint(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (mut v, mut o) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if v == o {
        return Ok(Value::from_bool(vm.len_of(v)? == 0));
    }
    // CPython's `dictviews_isdisjoint`: a longer set or view is probed by
    // the view's items.
    let setlike = |t: T| matches!(t, T::set | T::frozenset | T::dict_keys | T::dict_items);
    if setlike(vm.type_of(vm.unbx(o))) && vm.len_of(vm.unbx(o))? > vm.len_of(v)? {
        if vm.user_class(o).is_some() {
            return Err(vm.need_py());
        }
        (v, o) = (o, v);
    }
    // Lookups of keys with equal hashes are charged by the slots they
    // pass: past a step's work the twin goes on, a safepoint per item.
    let start = vm.work;
    for &x in &vm.collect(o)? {
        if vm.work.wrapping_sub(start) >= nstep::STEP_WORK {
            return Err(vm.need_py());
        }
        if vm.contains(v, x)? {
            return Ok(Value::FALSE);
        }
    }
    Ok(Value::TRUE)
}

fn dict_fromkeys(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    use crate::set::{act, phase};
    // A subclass builds an instance of itself (the prelude twin); the
    // receiver may be the class or an instance.
    let c = vm.arg_o(&a, 0);
    if c != vm.type_obj(T::dict) && vm.type_of(c) != T::dict {
        return Err(vm.need_py());
    }
    let keys = vm.bulk_src(vm.arg_o(&a, 1))?;
    let v = match vm.arg_o(&a, 2) {
        Value::UNDEFINED => Value::NONE,
        v => v,
    };
    let d = vm.new_dict()?;
    vm.bulk(&[d, keys, v], phase(act::KEYS, 0, 1, 2), 0, true)
}

fn set_update(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    set_update_many(vm, &a, mm_compile::ast::BinOp::BitOr, "update")
}

/// `a <= b` (`sub`) or `a >= b` for set `a`, `b` any iterable (made a
/// set first, as CPython does), in steps.
fn set_subset(vm: &mut Vm, a: Value, b: Value, sub: bool) -> Result<Value, Raised> {
    use crate::set::{act, phase, res};
    let (x, y) = if sub { (a, b) } else { (b, a) };
    if Vm::is_set(x) && Vm::is_set(y) {
        if vm.set_len(x) > vm.set_len(y) {
            return Ok(Value::FALSE);
        }
        return vm.bulk(&[x, y], phase(act::SUBSET, 0, 0, 1), res::TRUE, true);
    }
    let (src, items) = early_src(vm, b)?;
    if sub {
        // CPython's `len(a.intersection(b)) == len(a)`, the intersection
        // stopping once it holds all of `a`.
        let tmp = vm.new_set()?;
        let p = phase(act::AND | items, 2, 1, 0) | phase(act::SUBSET, 0, 0, 2) << 16;
        return vm.bulk(&[a, src, tmp], p, res::TRUE, true);
    }
    // Every item of `b` in `a`: the first missing one decides.
    vm.bulk(
        &[a, src],
        phase(act::SUBSET | items, 0, 1, 0),
        res::TRUE,
        true,
    )
}

fn set_issubset(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.check_anyset(&a, "issubset")?;
    set_subset(vm, s, vm.arg_o(&a, 1), true)
}

fn set_issuperset(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.check_anyset(&a, "issuperset")?;
    set_subset(vm, s, vm.arg_o(&a, 1), false)
}

fn set_copy(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    use crate::set::{act, phase};
    let s = vm.check_anyset(&a, "copy")?;
    if vm.set_frozen(s) {
        return Ok(s);
    }
    let out = vm.new_set()?;
    vm.bulk(&[out, s], phase(act::ADD, 0, 1, 0), 0, true)
}

fn set_clear(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.check_self(&a, T::set, "clear")?;
    vm.set_clear(s)?;
    Ok(Value::NONE)
}
