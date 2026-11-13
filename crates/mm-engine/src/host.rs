//! The host boundary: plain-data host values, host functions and host
//! modules, and conversions between host values and Python values
//! (docs/spec.md §10.1).
//!
//! A host function is an ordinary callable value. Calling it converts the
//! arguments to [`HostValue`]s (a value that cannot cross raises `TypeError`
//! before any round trip) and stops the dispatch loop with
//! [`Exit::HostCall`](crate::Exit::HostCall); every frame stays where it is,
//! so the call can happen at any depth.

use crate::Exit;
use crate::types::{T, ty};
use crate::vm::{Raised, Vm, exc_slot};
use alloc::{format, string::String, sync::Arc, vec::Vec};
use mm_compile::bytecode::Code;
use mm_core::Value;

/// Deepest nesting a host value may have in either direction, by default.
pub const MAX_HOST_DEPTH: u32 = 100;

/// The ceiling on [`HostArgs::max_depth`]: conversion recurses once per
/// level on the Rust stack.
pub(crate) const MAX_HOST_DEPTH_CEILING: u32 = 1000;

/// How host-call arguments (and completion values) convert to
/// [`HostValue`]s.
///
/// Conversion charges a lower bound of each value's compact JSON size
/// against `max_bytes` before copying it, so a small object graph that
/// shares sub-objects cannot expand into an unbounded host value. The
/// tree's Rust heap bytes are reserved from the session's memory limit
/// while it is built: past it a strict conversion raises `MemoryError`
/// (a completion value then has no host form), a lenient one stops with
/// [`Unconverted::TooLarge`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostArgs {
    /// Largest (lower-bound) compact JSON size of one call's arguments, in
    /// bytes. A value past the platform's address space means no limit.
    pub max_bytes: u64,
    /// Deepest nesting; the arguments themselves are at depth 0. Capped at
    /// 1000.
    pub max_depth: u32,
    /// Report what does not convert in place as [`HostValue::Unconverted`]
    /// and let the host decide, instead of raising before the call.
    pub lenient: bool,
}

impl Default for HostArgs {
    /// Strict, 16 MiB, [`MAX_HOST_DEPTH`].
    fn default() -> Self {
        HostArgs {
            max_bytes: 16 << 20,
            max_depth: MAX_HOST_DEPTH,
            lenient: false,
        }
    }
}

/// Why part of a host-call argument did not convert ([`HostArgs::lenient`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unconverted {
    /// A value with no host form (a set, a function, ...): its type name
    /// and its `repr()`, cut short when the byte budget runs out.
    Opaque {
        /// `type(value).__name__`.
        type_name: String,
        /// `repr(value)`.
        repr: String,
    },
    /// A dict key that is not a `str`; the dict's remaining entries and
    /// everything after it are left out.
    KeyNotStr,
    /// A value nested deeper than [`HostArgs::max_depth`]; everything after
    /// it is left out.
    TooDeep,
    /// The byte budget ran out at a value of this type; everything after
    /// it is left out.
    TooLarge {
        /// `type(value).__name__`.
        type_name: String,
    },
}

/// Plain data that crosses the host boundary, copied by value.
///
/// Not exhaustive: a host matching on it keeps a wildcard arm, so a later
/// variant (a set, say) is not a breaking change.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum HostValue {
    /// `None`.
    None,
    /// A bool.
    Bool(bool),
    /// An int that fits `i64`.
    Int(i64),
    /// An int outside `i64`, in decimal.
    BigInt(String),
    /// A float.
    Float(f64),
    /// A str.
    Str(String),
    /// Bytes.
    Bytes(Vec<u8>),
    /// A list.
    List(Vec<HostValue>),
    /// A tuple (a named tuple is a [`HostValue::Record`]).
    Tuple(Vec<HostValue>),
    /// A dict with `str` keys, in insertion order.
    Dict(Vec<(String, HostValue)>),
    /// A named tuple: its type name and fields in order. The names follow
    /// `collections.namedtuple`'s rule without `rename`: each an identifier
    /// and not a keyword, field names not starting with `_` and not
    /// repeated. Passed to Python with any other, it raises `TypeError`
    /// with `namedtuple`'s message.
    Record {
        /// The type name.
        name: String,
        /// Field names and values.
        fields: Vec<(String, HostValue)>,
    },
    /// An instance of a host class (`HostModule::classes` in the facade): its
    /// registered name and its fields in order. Value semantics: an
    /// immutable copy, never a reference to host state.
    Object {
        /// The class name as registered (`module.Class` or `Class`).
        class: String,
        /// Field names and values.
        fields: Vec<(String, HostValue)>,
    },
    /// Part of a host-call argument that did not convert; only with
    /// [`HostArgs::lenient`], and never accepted back.
    Unconverted(Unconverted),
}

impl HostValue {
    /// Owned buffers after cloning, whose capacities equal their lengths.
    /// Check depth before the derived Clone can recurse on the Rust stack.
    fn clone_bytes(&self, depth: u32, max_depth: &mut u32) -> Option<u64> {
        if matches!(
            self,
            Self::List(_)
                | Self::Tuple(_)
                | Self::Dict(_)
                | Self::Record { .. }
                | Self::Object { .. }
        ) {
            if depth >= MAX_HOST_DEPTH_CEILING {
                return None;
            }
            *max_depth = (*max_depth).max(depth + 1);
        }
        let mut fields = |fields: &[(String, HostValue)]| {
            fields
                .iter()
                .try_fold(size_of_val(fields) as u64, |bytes, (key, value)| {
                    Some(bytes + key.len() as u64 + value.clone_bytes(depth + 1, max_depth)?)
                })
        };
        Some(match self {
            Self::BigInt(s) | Self::Str(s) => s.len() as u64,
            Self::Bytes(b) => b.len() as u64,
            Self::List(items) | Self::Tuple(items) => items.iter().try_fold(
                (items.len() * size_of::<HostValue>()) as u64,
                |bytes, value| Some(bytes + value.clone_bytes(depth + 1, max_depth)?),
            )?,
            Self::Dict(items) => fields(items)?,
            Self::Record {
                name,
                fields: items,
            }
            | Self::Object {
                class: name,
                fields: items,
            } => name.len() as u64 + fields(items)?,
            Self::Unconverted(Unconverted::Opaque { type_name, repr }) => {
                (type_name.len() + repr.len()) as u64
            }
            Self::Unconverted(Unconverted::TooLarge { type_name }) => type_name.len() as u64,
            _ => 0,
        })
    }
}

/// The Python exception class a host error raises.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum HostErrorKind {
    /// `Exception` (the default).
    #[default]
    Exception,
    /// `ValueError`.
    ValueError,
    /// `TypeError`.
    TypeError,
    /// `KeyError`.
    KeyError,
    /// `IndexError`.
    IndexError,
    /// `LookupError`.
    LookupError,
    /// `AttributeError`.
    AttributeError,
    /// `RuntimeError`.
    RuntimeError,
    /// `NotImplementedError`.
    NotImplementedError,
    /// `OSError`.
    OSError,
    /// `TimeoutError`.
    TimeoutError,
    /// `ZeroDivisionError`.
    ZeroDivisionError,
    /// `NameError`.
    NameError,
    /// `PermissionError`.
    PermissionError,
    /// `FileNotFoundError`.
    FileNotFoundError,
    /// `FileExistsError`.
    FileExistsError,
    /// `IsADirectoryError`.
    IsADirectoryError,
    /// `NotADirectoryError`.
    NotADirectoryError,
}

/// The kind ↔ class table.
const KINDS: [(HostErrorKind, T); 18] = [
    (HostErrorKind::Exception, T::Exception),
    (HostErrorKind::ValueError, T::ValueError),
    (HostErrorKind::TypeError, T::TypeError),
    (HostErrorKind::KeyError, T::KeyError),
    (HostErrorKind::IndexError, T::IndexError),
    (HostErrorKind::LookupError, T::LookupError),
    (HostErrorKind::AttributeError, T::AttributeError),
    (HostErrorKind::RuntimeError, T::RuntimeError),
    (HostErrorKind::NotImplementedError, T::NotImplementedError),
    (HostErrorKind::OSError, T::OSError),
    (HostErrorKind::TimeoutError, T::TimeoutError),
    (HostErrorKind::ZeroDivisionError, T::ZeroDivisionError),
    (HostErrorKind::NameError, T::NameError),
    (HostErrorKind::PermissionError, T::PermissionError),
    (HostErrorKind::FileNotFoundError, T::FileNotFoundError),
    (HostErrorKind::FileExistsError, T::FileExistsError),
    (HostErrorKind::IsADirectoryError, T::IsADirectoryError),
    (HostErrorKind::NotADirectoryError, T::NotADirectoryError),
];

impl HostErrorKind {
    /// The kind for a Python class name (`"ValueError"`), if listed.
    #[must_use]
    pub fn from_name(name: &str) -> Option<HostErrorKind> {
        KINDS
            .iter()
            .find(|(_, t)| t.name() == name)
            .map(|(k, _)| *k)
    }

    /// The Python class name.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::class(self).name()
    }

    fn class(self) -> T {
        KINDS
            .iter()
            .find(|(k, _)| *k == self)
            .map_or(T::Exception, |(_, t)| *t)
    }
}

/// A failed host call: raised in Python as `kind(message)`, or as an
/// instance of a host exception class.
///
/// With [`HostError::class`] or [`HostError::data`] set, the exception
/// also has the attributes `tool` (the call name) and `data` (the payload,
/// or `None`).
#[derive(Clone, Debug, PartialEq)]
pub struct HostError {
    /// The exception class, unless [`HostError::class`] names one.
    pub kind: HostErrorKind,
    /// The message (the exception's single argument).
    pub message: String,
    /// A payload: the exception's `data` attribute.
    pub data: Option<HostValue>,
    /// A host exception class (the facade's `HostModule::exceptions`) by
    /// its registered name, `module.Name` or `Name` for the builtins
    /// module; it replaces `kind`.
    pub class: Option<String>,
}

impl HostError {
    /// `kind(message)`, with no payload or host class.
    #[must_use]
    pub fn new(kind: HostErrorKind, message: impl Into<String>) -> HostError {
        HostError {
            kind,
            message: message.into(),
            data: None,
            class: None,
        }
    }
}

/// What became of a host call ([`Vm::call_log`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CallStatus {
    /// The host returned a value.
    Returned,
    /// The host raised an error.
    Raised,
    /// The host has not answered yet: the cell is stopped at the call, or
    /// waits on its future.
    Pending,
    /// The cell ended (completed, raised or was cancelled) before the
    /// host answered: an unawaited or abandoned future.
    Abandoned,
}

impl CallStatus {
    const ALL: [CallStatus; 4] = [
        CallStatus::Pending,
        CallStatus::Returned,
        CallStatus::Raised,
        CallStatus::Abandoned,
    ];

    pub(crate) fn code(self) -> u8 {
        match self {
            CallStatus::Pending => 0,
            CallStatus::Returned => 1,
            CallStatus::Raised => 2,
            CallStatus::Abandoned => 3,
        }
    }

    pub(crate) fn from_code(c: u8) -> Option<CallStatus> {
        Self::ALL.get(c as usize).copied()
    }
}

/// One host call of the cell: `(call id, function id, status)`.
pub(crate) type CallEntry = (u64, u32, CallStatus);

/// A call of a host function, as the engine reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct HostCall {
    /// The call id; increases monotonically per session.
    pub id: u64,
    /// The function's id ([`Vm::host_id`]): stable per name for the
    /// session's lifetime, snapshots included.
    pub function: u32,
    /// The function: its registered name, or `module.name` for a host
    /// module's function.
    pub name: String,
    /// Positional arguments.
    pub args: Vec<HostValue>,
    /// Keyword arguments, in call order.
    pub kwargs: Vec<(String, HostValue)>,
}

/// Host-module code compiled once and shared by sessions: what
/// [`Vm::run_host_defs`] runs, kept by the embedder's [`crate::ModuleCache`]
/// under `key`. A snapshot refers to the code of a module's current
/// definition by `key` instead of storing it, and its restore asks the
/// embedder for it again (with the module's spec and `recipe`).
pub struct HostProgram {
    /// Identifies the program: the embedder's hash of its source.
    pub key: [u8; 16],
    /// The compiled program.
    pub program: mm_compile::bytecode::Program,
    /// [`HostProgram::code_hash`] of `program`. A snapshot keeps it beside
    /// the key, and a restore refuses a program built again from the same
    /// source that compiled to other code (the build id is meant to rule
    /// that out; this checks it).
    pub code: u64,
    /// What else, besides the module's spec, the embedder needs to build
    /// it again; saved with snapshots.
    pub recipe: Vec<String>,
    /// [`HostProgram::definition_hash`] of the definition it was built
    /// from. A restore takes the cached program for an image's key only
    /// when the image's definition has this hash; otherwise the embedder
    /// builds it again from the image's definition, which must give the
    /// key. (Definitions that differ only where the generated source does
    /// not, such as docstrings, share a key.)
    pub definition: u64,
}

impl HostProgram {
    /// A hash of module `name`'s definition (`spec`, `recipe`) as a
    /// snapshot stores it.
    #[must_use]
    pub fn definition_hash(name: &str, spec: &HostValue, recipe: &[String]) -> u64 {
        let mut bytes = Vec::new();
        crate::snapshot::put_host(&mut bytes, spec);
        definition_hash_of(name, &bytes, recipe)
    }
    /// The bytes it keeps alive.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.program
            .codes
            .iter()
            .map(|c| c.buffer_bytes() + size_of::<Code>() as u64)
            .sum::<u64>()
            + self.recipe.iter().map(|r| r.len() as u64).sum::<u64>()
    }
}

/// [`HostProgram::definition_hash`] of a spec already encoded (`spec`, as
/// a snapshot stores it).
pub(crate) fn definition_hash_of(name: &str, spec: &[u8], recipe: &[String]) -> u64 {
    // XXH64's four lanes hash a long spec several times faster than
    // `hash_bytes`' one chain.
    let mut h = mm_core::xxh64(spec, mm_core::xxh64(name.as_bytes(), 0));
    for r in recipe {
        h = mm_core::xxh64(r.as_bytes(), h);
    }
    h
}

/// A host module registered by the embedder.
pub(crate) struct HostModule {
    pub name: String,
    pub functions: Vec<String>,
    pub constants: Vec<(String, HostValue)>,
    /// The embedder's definition, kept for it (the facade's stubs).
    pub spec: HostValue,
    /// The code its definition ran, if any.
    pub program: Option<Arc<HostProgram>>,
    /// Arc allocation and exact-sized, immutable child buffers.
    pub bytes: u64,
    /// Reserve enough traversal workspace to restore at the same limit.
    pub scratch: u64,
}

/// `HOSTFN` slots.
pub(crate) mod hostfn_slot {
    /// The name host calls carry.
    pub const CALL_NAME: u32 = 3;
    /// The raw word with the function id.
    pub const ID: u32 = 4;
}

impl Vm {
    /// The id of host function `call_name`: the one it had, else the next.
    ///
    /// # Errors
    ///
    /// A pending `MemoryError` if the id table cannot grow.
    pub fn host_id(&mut self, call_name: &str) -> Result<u32, Raised> {
        if let Some(id) = self.find_host_id(call_name) {
            return Ok(id);
        }
        let grow = if self.host_ids.len() == self.host_ids.capacity() {
            self.host_ids.len().max(4)
        } else {
            0
        };
        self.reserve_code((call_name.len() + grow * size_of::<String>()) as u64)?;
        let pushed = self.host_ids.try_reserve_exact(grow).is_ok();
        if pushed {
            self.host_ids.push(call_name.into());
        }
        self.reserve_code(0)?;
        if !pushed {
            return Err(self.err(T::MemoryError, ""));
        }
        Ok(u32::try_from(self.host_ids.len() - 1).unwrap_or(u32::MAX))
    }

    /// The id host function `call_name` has, if it has one.
    #[must_use]
    pub fn find_host_id(&self, call_name: &str) -> Option<u32> {
        let i = self.host_ids.iter().position(|n| **n == *call_name)?;
        u32::try_from(i).ok()
    }

    /// The call name of host function id `id`.
    #[must_use]
    pub fn host_id_name(&self, id: u32) -> Option<&str> {
        self.host_ids.get(id as usize).map(String::as_str)
    }

    /// Registers host module `name` (dotted names are submodules, reachable
    /// as attributes of their parent; `""` is the builtins) with these
    /// attributes (host functions, or what [`Vm::run_host_defs`] defined
    /// under `name.attr`) and constants, replacing any earlier definition
    /// whole. `spec` is kept for the embedder. Survives `reset`; an
    /// imported copy is rebuilt.
    pub fn define_host_module(
        &mut self,
        name: &str,
        functions: &[&str],
        constants: &[(&str, HostValue)],
        spec: &HostValue,
        program: Option<&Arc<HostProgram>>,
    ) -> Result<(), Raised> {
        let mut bytes = (size_of::<HostModule>()
            + 2 * size_of::<usize>()
            + name.len()
            + functions.len() * size_of::<String>()
            + constants.len() * size_of::<(String, HostValue)>()) as u64
            + functions.iter().map(|f| f.len() as u64).sum::<u64>();
        let mut max_depth = 0;
        for (key, value) in constants {
            let Some(value_bytes) = value.clone_bytes(0, &mut max_depth) else {
                return Err(self.err(
                    T::RecursionError,
                    "maximum recursion depth exceeded while registering a host module",
                ));
            };
            bytes += key.len() as u64 + value_bytes;
        }
        let Some(spec_bytes) = spec.clone_bytes(0, &mut max_depth) else {
            return Err(self.err(
                T::RecursionError,
                "maximum recursion depth exceeded while registering a host module",
            ));
        };
        bytes += spec_bytes;
        let scratch = u64::from(max_depth) * crate::snapshot::HOST_FRAME_BYTES;
        let reserved_scratch = self
            .host_modules
            .iter()
            .map(|m| m.scratch)
            .max()
            .unwrap_or(0);
        let grow = self.host_modules.len() == self.host_modules.capacity()
            && !self.host_modules.iter().any(|m| m.name == name);
        // The old definition remains live until replacement succeeds. Charge
        // the temporary new clone as well, not just the eventual size difference.
        self.reserve_code(
            bytes
                + scratch.saturating_sub(reserved_scratch)
                + if grow {
                    size_of::<Arc<HostModule>>() as u64
                } else {
                    0
                },
        )?;
        let result = (|| {
            // Intern all prefixes before changing any imported module. Slices
            // avoid a second, growing copy of a dotted module name.
            for end in name
                .match_indices('.')
                .map(|(i, _)| i)
                .chain(core::iter::once(name.len()))
            {
                self.intern(&name[..end])?;
            }
            if grow {
                self.host_modules
                    .try_reserve_exact(1)
                    .map_err(|_| self.err(T::MemoryError, "memory limit exceeded"))?;
            }
            let module = Arc::new(HostModule {
                name: name.into(),
                functions: functions.iter().map(|f| String::from(*f)).collect(),
                constants: constants
                    .iter()
                    .map(|(k, v)| (String::from(*k), v.clone()))
                    .collect(),
                spec: spec.clone(),
                program: program.cloned(),
                bytes,
                scratch,
            });
            for end in name
                .match_indices('.')
                .map(|(i, _)| i)
                .chain(core::iter::once(name.len()))
            {
                let key = self.intern(&name[..end])?;
                self.dict_del(self.modules, key)?;
            }
            let old = self.host_modules.iter().position(|m| m.name == name);
            let old = old.map(|i| self.host_modules.remove(i));
            self.host_modules.push(module);
            if name.is_empty() {
                // The builtins module: its attributes are builtins.
                let b = self.builtins;
                if let Some(old) = old {
                    for k in old
                        .functions
                        .iter()
                        .chain(old.constants.iter().map(|(k, _)| k))
                    {
                        let k = self.intern(k)?;
                        self.dict_del(b, k)?;
                    }
                }
                for f in functions {
                    let v = self.new_hostfn(f, f)?;
                    let k = self.intern(f)?;
                    self.dict_set(b, k, v)?;
                }
                for (k, h) in constants {
                    let v = self.py_value(h)?;
                    let k = self.intern(k)?;
                    self.dict_set(b, k, v)?;
                }
            }
            self.ics.fill(crate::vm::Ic::Empty);
            Ok(())
        })();
        // Release the temporary reservation after either commit or rollback.
        let _ = self.reserve_code(0);
        result
    }

    /// The host function `call_name` reports, as `name`: what the embedder
    /// defined under `call_name` with a signature or as a class
    /// ([`Vm::run_host_defs`]), else a plain host function.
    pub(crate) fn new_hostfn(&mut self, name: &str, call_name: &str) -> Result<Value, Raised> {
        match self.host_def(call_name) {
            Some(v) => Ok(v),
            None => self.raw_hostfn(name, call_name),
        }
    }

    /// The value defined for `key` in the prelude's `_host_defs`.
    #[inline(never)]
    fn host_def(&self, key: &str) -> Option<Value> {
        let defs = self.dict_get_str(self.prelude_globals, self.find_interned("_host_defs")?)?;
        self.dict_get_str(defs, self.find_interned(key)?)
    }

    /// A host function that is not looked up in `_host_defs`.
    pub fn raw_hostfn(&mut self, name: &str, call_name: &str) -> Result<Value, Raised> {
        let id = self.host_id(call_name)?;
        let n = self.intern(name)?;
        let c = self.intern(call_name)?;
        let r = {
            let res =
                self.heap
                    .alloc_init(ty::HOSTFN, 0, &[n, n, Value::NONE, c], &[u64::from(id)]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::HOSTFN as u8))
    }

    /// The host call for `callee(args...)`: converts the arguments or raises.
    /// After a lenient conversion stops, the remaining arguments are `None`
    /// placeholders, so the host still sees how many there were.
    pub(crate) fn host_call(
        &mut self,
        callee: Value,
        args: usize,
        npos: usize,
        kwnames: Value,
    ) -> Result<HostCall, Raised> {
        let name = String::from(self.str_ref(self.vslot(callee, hostfn_slot::CALL_NAME)));
        let function =
            u32::try_from(self.vslot_raw_of(callee, hostfn_slot::ID)).unwrap_or(u32::MAX);
        let mut conv = Conversion::new(self.host_args);
        // The argument vectors are charged with the trees: a starred call
        // passes as many arguments as the memory limit holds, and each
        // host value is several times its stack slot.
        let nkw = if kwnames == Value::NONE {
            0
        } else {
            self.seq_words(kwnames).len()
        };
        let slots = npos
            .saturating_mul(size_of::<HostValue>())
            .saturating_add(nkw.saturating_mul(size_of::<(String, HostValue)>()));
        if !self.hold(&mut conv, slots) {
            return Err(self.err(T::MemoryError, ""));
        }
        let mut pos = Vec::with_capacity(npos);
        for i in 0..npos {
            let v = self.stack[args + i];
            pos.push(self.host_arg(v, &name, &mut conv)?);
        }
        let mut kwargs = Vec::new();
        if kwnames != Value::NONE {
            for (k, n) in self.tuple_items(kwnames).into_iter().enumerate() {
                let v = self.stack[args + npos + k];
                let hv = self.host_arg(v, &name, &mut conv)?;
                kwargs.push((String::from(self.str_ref(n)), hv));
            }
        }
        let id = self.next_call;
        self.log_call(id, function)?;
        self.next_call += 1;
        Ok(HostCall {
            id,
            function,
            name,
            args: pos,
            kwargs,
        })
    }

    fn host_arg(
        &mut self,
        v: Value,
        func: &str,
        conv: &mut Conversion,
    ) -> Result<HostValue, Raised> {
        if conv.stopped.is_some() {
            return Ok(HostValue::None);
        }
        self.host_value(v, func, conv)
    }

    /// Sets how host-call arguments and completion values convert.
    pub fn set_host_args(&mut self, args: HostArgs) {
        self.host_args = HostArgs {
            max_depth: args.max_depth.min(MAX_HOST_DEPTH_CEILING),
            ..args
        };
    }

    /// `v` as a host value. A strict conversion raises `TypeError`,
    /// `RecursionError` or `ValueError` (the byte budget); a lenient one
    /// puts an [`Unconverted`] in place, and containers stop at the first
    /// one that is not [`Unconverted::Opaque`].
    fn host_value(
        &mut self,
        v: Value,
        func: &str,
        conv: &mut Conversion,
    ) -> Result<HostValue, Raised> {
        // One loop over the containers open, so nesting up to `max_depth`
        // (at most 1000) costs charged heap, not Rust stack.
        let mut open: Vec<Open> = Vec::new();
        let mut next = Some(v);
        let mut done = None;
        loop {
            if let Some(x) = next.take() {
                let depth = u32::try_from(open.len()).unwrap_or(u32::MAX);
                match self.host_open(x, func, depth, conv)? {
                    Opened::Value(h) => done = Some(h),
                    Opened::Open(o) => open.push(o),
                }
            }
            let Some(top) = open.last_mut() else {
                return Ok(done.unwrap_or(HostValue::None));
            };
            if let Some(h) = done.take() {
                top.accept(h);
            }
            match self.host_next(top, func, conv)? {
                Some(x) => next = Some(x),
                None => {
                    if let Some(o) = open.pop() {
                        conv.held = conv.held.saturating_sub(o.copies());
                        done = Some(o.finish());
                    }
                }
            }
        }
    }

    /// Starts converting `x` at nesting `depth`: its value, or the
    /// container to fill.
    fn host_open(
        &mut self,
        x: Value,
        func: &str,
        depth: u32,
        conv: &mut Conversion,
    ) -> Result<Opened, Raised> {
        if depth > conv.args.max_depth {
            return self.host_too_deep(conv).map(Opened::Value);
        }
        if let Some(r) = self.host_leaf(x, func, conv) {
            return r.map(Opened::Value);
        }
        // Each container charges its slots, the items copied out for its
        // level and its place on the open stack.
        let frame = size_of::<Open>();
        if let Some((c, back, fields)) = self.record_parts(x) {
            // A named tuple: a `Record` (charged as a dict of its fields).
            let n = self.seq_words(back).len();
            let copies = frame + n * (size_of::<Value>() + size_of::<String>());
            let slots = n * size_of::<(String, HostValue)>();
            let cost = n.saturating_sub(1) + 2;
            if let Some(marker) = self.charge(x, func, conv, cost, slots + copies)? {
                return Ok(Opened::Value(marker));
            }
            let (name, names, items, class) = self.record_copy(c, back, fields);
            let out = Vec::with_capacity(items.len().min(conv.room()));
            return Ok(Opened::Open(Open::Record {
                x,
                fields: names.into_iter().zip(items),
                out,
                key: String::new(),
                name,
                class,
                copies,
            }));
        }
        if x.is_obj_kind(ty::LIST as u8) || x.is_obj_kind(ty::TUPLE as u8) {
            let n = self.seq_words(x).len();
            let copies = frame + n * size_of::<Value>();
            let slots = n * size_of::<HostValue>();
            let cost = n.saturating_sub(1) + 2;
            if let Some(marker) = self.charge(x, func, conv, cost, slots + copies)? {
                return Ok(Opened::Value(marker));
            }
            let items = self.seq_items(x).unwrap_or_default();
            let out = Vec::with_capacity(items.len().min(conv.room()));
            return Ok(Opened::Open(Open::Seq {
                tuple: x.is_obj_kind(ty::TUPLE as u8),
                items: items.into_iter(),
                out,
                copies,
            }));
        }
        if x.is_obj_kind(ty::DICT as u8) {
            let n = self.dict_live_len(x);
            let copies = frame + n * size_of::<(Value, Value)>();
            let slots = n * size_of::<(String, HostValue)>();
            let cost = n.saturating_sub(1) + 2;
            if let Some(marker) = self.charge(x, func, conv, cost, slots + copies)? {
                return Ok(Opened::Value(marker));
            }
            let items = self.dict_items_vec(x);
            let out = Vec::with_capacity(items.len().min(conv.room()));
            return Ok(Opened::Open(Open::Dict {
                items: items.into_iter(),
                out,
                key: String::new(),
                copies,
            }));
        }
        self.host_opaque(x, func, conv).map(Opened::Value)
    }

    /// The next item of `o` to convert, or `None` once `o` is complete
    /// (all items, or a stop marker in a lenient conversion).
    fn host_next(
        &mut self,
        o: &mut Open,
        func: &str,
        conv: &mut Conversion,
    ) -> Result<Option<Value>, Raised> {
        if conv.stopped.is_some() {
            return Ok(None);
        }
        match o {
            Open::Seq { items, .. } => Ok(items.next()),
            Open::Dict {
                items, out, key, ..
            } => {
                let Some((k, x)) = items.next() else {
                    return Ok(None);
                };
                if !Vm::is_str(k) {
                    let marker = self.host_key_not_str(k, func, conv)?;
                    out.push((String::new(), marker));
                    return Ok(None);
                }
                let n = self.str_ref(k).len();
                if let Some(marker) = self.charge(k, func, conv, n + 3, n)? {
                    out.push((String::new(), marker));
                    return Ok(None);
                }
                *key = String::from(self.str_ref(k));
                Ok(Some(x))
            }
            Open::Record {
                x: rec,
                fields,
                out,
                key,
                ..
            } => {
                let Some((k, x)) = fields.next() else {
                    return Ok(None);
                };
                if let Some(marker) = self.charge(*rec, func, conv, k.len() + 3, k.len())? {
                    out.push((String::new(), marker));
                    return Ok(None);
                }
                *key = k;
                Ok(Some(x))
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn host_too_deep(&mut self, conv: &mut Conversion) -> Result<HostValue, Raised> {
        conv.stop(Unconverted::TooDeep, || {
            self.err(
                T::RecursionError,
                "maximum recursion depth exceeded while converting a host value",
            )
        })
    }

    /// [`Vm::host_value`] of a value that is not a container, or `None`.
    #[inline(never)]
    fn host_leaf(
        &mut self,
        v: Value,
        func: &str,
        conv: &mut Conversion,
    ) -> Option<Result<HostValue, Raised>> {
        let leaf = if v == Value::NONE {
            Some((HostValue::None, 4))
        } else if v == Value::TRUE || v == Value::FALSE {
            Some((HostValue::Bool(v == Value::TRUE), 4))
        } else if let Some(i) = self.int_of(v) {
            Some((HostValue::Int(i), decimal_len(i)))
        } else {
            v.as_float().map(|f| (HostValue::Float(f), 1))
        };
        if let Some((h, cost)) = leaf {
            return Some(self.charged(v, func, conv, cost, |_| h));
        }
        if let Some(b) = self.big_of(v).filter(|_| v.is_obj_kind(ty::INT as u8)) {
            // CPython's 4300-digit limit, checked before the quadratic
            // conversion (as `str(v)` does).
            let text = match self.big_decimal(&b) {
                Ok(t) => t,
                Err(e) => return Some(Err(e)),
            };
            return Some(self.charged(v, func, conv, text.len(), |_| HostValue::BigInt(text)));
        }
        if v.is_obj_kind(ty::BYTES as u8) {
            let cost = self.bytes_data(v).len() + 3;
            return Some(self.charged(v, func, conv, cost, |vm| {
                HostValue::Bytes(vm.bytes_data(v).to_vec())
            }));
        }
        if Vm::is_str(v) {
            let cost = self.str_ref(v).len() + 2;
            return Some(self.charged(v, func, conv, cost, |vm| {
                HostValue::Str(String::from(vm.str_ref(v)))
            }));
        }
        None
    }

    /// A value with no host form: `TypeError`, or (lenient) its capped repr.
    #[inline(never)]
    fn host_opaque(
        &mut self,
        v: Value,
        func: &str,
        conv: &mut Conversion,
    ) -> Result<HostValue, Raised> {
        if !conv.args.lenient {
            return Err(self.err(
                T::TypeError,
                &format!(
                    "{func}() argument: a '{}' object cannot be passed to the host",
                    self.type_name(v)
                ),
            ));
        }
        let repr = self.repr_capped(v, conv.room())?;
        let type_name = String::from(self.type_name(v));
        self.charged(v, func, conv, repr.len() + 2, |_| {
            HostValue::Unconverted(Unconverted::Opaque {
                type_name,
                repr: repr.text,
            })
        })
    }

    #[cold]
    #[inline(never)]
    fn host_key_not_str(
        &mut self,
        k: Value,
        func: &str,
        conv: &mut Conversion,
    ) -> Result<HostValue, Raised> {
        conv.stop(Unconverted::KeyNotStr, || {
            self.err(
                T::TypeError,
                &format!(
                    "{func}() argument: dict keys passed to the host must be str, not {}",
                    self.type_name(k)
                ),
            )
        })
    }

    /// `make(self)` after charging `cost` for `v`, or the stop marker.
    fn charged(
        &mut self,
        v: Value,
        func: &str,
        conv: &mut Conversion,
        cost: usize,
        make: impl FnOnce(&Vm) -> HostValue,
    ) -> Result<HostValue, Raised> {
        match self.charge(v, func, conv, cost, cost)? {
            Some(marker) => Ok(marker),
            None => Ok(make(self)),
        }
    }

    #[inline(never)]
    /// Charges `cost` bytes for `v` against the byte budget and `held`
    /// bytes of Rust heap against the session's memory budget. Past the
    /// byte budget a strict conversion raises `ValueError`, past the
    /// memory budget `MemoryError`; a lenient one returns the stop marker.
    fn charge(
        &mut self,
        v: Value,
        func: &str,
        conv: &mut Conversion,
        cost: usize,
        held: usize,
    ) -> Result<Option<HostValue>, Raised> {
        if let Some(rest) = conv.budget.checked_sub(cost) {
            if self.hold(conv, held) {
                conv.budget = rest;
                return Ok(None);
            }
            let type_name = String::from(self.type_name(v));
            return conv
                .stop(Unconverted::TooLarge { type_name }, || {
                    self.err(T::MemoryError, "")
                })
                .map(Some);
        }
        let max = conv.args.max_bytes;
        let type_name = String::from(self.type_name(v));
        conv.stop(Unconverted::TooLarge { type_name }, || {
            self.err(
                T::ValueError,
                &format!(
                    "{func}() argument: the value is too large to pass to the host (more than {max} bytes)"
                ),
            )
        })
        .map(Some)
    }

    /// Reserves `bytes` more of Rust heap for a conversion, in steps that
    /// at least double, so a big tree reserves a logarithmic number of
    /// times; `false` past the session's memory budget.
    fn hold(&mut self, conv: &mut Conversion, bytes: usize) -> bool {
        let Some(held) = conv.held.checked_add(bytes) else {
            return false;
        };
        conv.held = held;
        let have = conv.reserved.as_ref().map_or(0, mm_core::Scratch::bytes);
        if held <= have {
            return true;
        }
        let want = held.max(have.saturating_mul(2)).max(16 << 10);
        let res = match &mut conv.reserved {
            Some(r) => r
                .resize(&self.heap, want)
                .or_else(|_| r.resize(&self.heap, held)),
            None => self
                .heap
                .reserve_scratch(want)
                .or_else(|_| self.heap.reserve_scratch(held))
                .map(|r| conv.reserved = Some(r)),
        };
        res.is_ok()
    }

    /// `v` as a host value if it converts strictly within the byte budget
    /// and the session's memory (the completion value of a cell).
    pub fn value_to_host(&mut self, v: Value) -> Option<HostValue> {
        let saved = self.exc;
        let mut conv = Conversion::new(HostArgs {
            lenient: false,
            ..self.host_args
        });
        let r = self.host_value(v, "result", &mut conv).ok();
        self.exc = saved;
        r
    }

    /// A Python value for `h`, or a pending exception.
    pub fn py_value(&mut self, h: &HostValue) -> Result<Value, Raised> {
        self.py_value_depth(h, 0)
    }

    fn py_value_depth(&mut self, h: &HostValue, depth: u32) -> Result<Value, Raised> {
        if depth > MAX_HOST_DEPTH {
            return Err(self.err(
                T::RecursionError,
                "maximum recursion depth exceeded while converting a host value",
            ));
        }
        Ok(match h {
            HostValue::None => Value::NONE,
            HostValue::Bool(b) => Value::from_bool(*b),
            HostValue::Int(i) => self.new_int(*i)?,
            HostValue::BigInt(s) => {
                let (neg, digits) = match s.strip_prefix('-') {
                    Some(d) => (true, d),
                    None => (false, s.as_str()),
                };
                let parsed = (!digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()))
                    .then(|| crate::bigint::Big::parse(digits.as_bytes(), 10))
                    .flatten();
                match parsed {
                    Some(b) => self.new_big(&if neg { b.neg() } else { b })?,
                    None => {
                        return Err(self.err(
                            T::ValueError,
                            &format!(
                                "invalid host BigInt value: {}",
                                crate::text::debug_quoted(s)
                            ),
                        ));
                    }
                }
            }
            HostValue::Float(f) => Value::float(*f),
            HostValue::Str(s) => self.new_str(s)?,
            HostValue::Bytes(b) => self.new_bytes(b)?,
            HostValue::List(items) | HostValue::Tuple(items) => {
                let mut vals = Vec::with_capacity(items.len());
                for x in items {
                    vals.push(self.py_value_depth(x, depth + 1)?);
                }
                if matches!(h, HostValue::Tuple(_)) {
                    self.alloc_tuple(&vals)?
                } else {
                    self.new_list(&vals)?
                }
            }
            HostValue::Dict(items) => {
                let d = self.new_dict()?;
                for (k, x) in items {
                    let key = self.new_str(k)?;
                    let val = self.py_value_depth(x, depth + 1)?;
                    self.dict_set(d, key, val)?;
                }
                d
            }
            HostValue::Record { name, fields }
            | HostValue::Object {
                class: name,
                fields,
            } => {
                let mut vals = Vec::with_capacity(fields.len());
                for (_, x) in fields {
                    vals.push(self.py_value_depth(x, depth + 1)?);
                }
                let cls = if matches!(h, HostValue::Object { .. }) {
                    self.host_class(name, fields)?
                } else {
                    self.record_class(name, fields)?
                };
                let t = self.alloc_tuple(&vals)?;
                self.new_bx(cls, t)?
            }
            HostValue::Unconverted(_) => {
                return Err(self.err(
                    T::TypeError,
                    "an unconverted host value cannot be returned to Python",
                ));
            }
        })
    }

    /// A named tuple's class, backing tuple and `_fields`, checked without
    /// copying: an instance of a `tuple` subclass whose `_fields` is a tuple
    /// of `str`, one per item.
    fn record_parts(&self, v: Value) -> Option<(Value, Value, Value)> {
        let c = self.user_class(v)?;
        let back = self.backing(v).filter(|b| b.is_obj_kind(ty::TUPLE as u8))?;
        let key = self.find_interned("_fields")?;
        let fields = self.class_lookup(c, key)?;
        if !fields.is_obj_kind(ty::TUPLE as u8) {
            return None;
        }
        let names = self.seq_words(fields);
        if names.len() != self.seq_words(back).len()
            || !names
                .iter()
                .all(|&n| Value::from_bits(n).is_some_and(Vm::is_str))
        {
            return None;
        }
        Some((c, back, fields))
    }

    /// The class name, field names and items of the record `record_parts`
    /// checked. Last, the registered name of a host class
    /// (`__host_class__`).
    #[allow(clippy::type_complexity, reason = "one private caller")]
    fn record_copy(
        &self,
        c: Value,
        back: Value,
        fields: Value,
    ) -> (String, Vec<String>, Vec<Value>, Option<String>) {
        let names = self
            .tuple_items(fields)
            .iter()
            .map(|n| String::from(self.str_ref(*n)))
            .collect();
        let items = self.tuple_items(back);
        let class = self
            .find_interned("__host_class__")
            .and_then(|k| self.class_lookup(c, k))
            .filter(|n| Vm::is_str(*n))
            .map(|n| String::from(self.str_ref(n)));
        (String::from(self.class_name(c)), names, items, class)
    }

    /// The host class `name` (`HostModule::classes` in the facade), if `fields`
    /// are its fields in order.
    fn host_class(&mut self, name: &str, fields: &[(String, HostValue)]) -> Result<Value, Raised> {
        let declared = self
            .host_def(name)
            .filter(|c| Vm::is_class(*c))
            .zip(self.find_interned("_fields"))
            .and_then(|(c, k)| Some((c, self.class_lookup(c, k)?)))
            .filter(|(_, f)| f.is_obj_kind(ty::TUPLE as u8));
        let Some((cls, names)) = declared else {
            return Err(self.err(T::TypeError, "unknown host class"));
        };
        let names = self.tuple_items(names);
        if names.len() != fields.len()
            || names
                .iter()
                .zip(fields)
                .any(|(n, (k, _))| self.str_ref(*n) != k)
        {
            return Err(self.err(
                T::TypeError,
                "host object fields differ from its class's, in order",
            ));
        }
        Ok(cls)
    }

    /// The named tuple class for a host `Record` called `name` with these
    /// fields: a `_HostRecord` subclass, one per name and field list.
    fn record_class(
        &mut self,
        name: &str,
        fields: &[(String, HostValue)],
    ) -> Result<Value, Raised> {
        let mut key = Vec::with_capacity(fields.len() + 1);
        key.push(self.new_str(name)?);
        for (k, _) in fields {
            key.push(self.new_str(k)?);
        }
        let key = self.alloc_tuple(&key)?;
        let cache = self.prelude_fn("_host_records")?;
        if let Some(c) = self.dict_get(cache, key)? {
            return Ok(c);
        }
        // Only valid names reach the cache: `namedtuple`'s rule, without
        // renaming (a host bug is reported, not hidden).
        let fields: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
        if let Some(msg) = record_names_error(name, &fields) {
            return Err(self.err(T::TypeError, &msg));
        }
        let names = self.tuple_items(key);
        let field_names = self.alloc_tuple(names.get(1..).unwrap_or_default())?;
        let ns = self.new_dict()?;
        let k = self.intern("_fields")?;
        self.dict_set(ns, k, field_names)?;
        let k = self.intern("__module__")?;
        let m = self.new_str("__main__")?;
        self.dict_set(ns, k, m)?;
        for (attr, f) in [
            ("__getattr__", "_rec_getattr"),
            ("__repr__", "_rec_repr"),
            ("_asdict", "_rec_asdict"),
            ("_replace", "_rec_replace"),
            ("_make", "_rec_make"),
            ("__getnewargs__", "_rec_getnewargs"),
        ] {
            let mut v = self.prelude_fn(f)?;
            if attr == "_make" {
                v = self.wrap_descr(ty::CLASSMETHOD, v)?;
            }
            let k = self.intern(attr)?;
            self.dict_set(ns, k, v)?;
        }
        let cname = names.first().copied().unwrap_or(Value::NONE);
        let tuple = self.type_obj(T::tuple);
        let c = self.new_class(cname, &[tuple], ns)?;
        self.dict_set(cache, key, c)?;
        Ok(c)
    }

    /// Replaces what generated Python defines for host module `module`
    /// (`""` for the builtins): drops the `_host_defs` entries under it,
    /// then, with `prog` (ending in an expression that evaluates to a dict
    /// of `key: value`), loads its shared code, runs it in the prelude's
    /// globals, hidden from tracebacks, with `_host_raw` (a list of plain
    /// host functions made from `raws`, `(call name, name)` pairs),
    /// `_host_msgs` and `_host_data` bound, and keeps every entry. Then
    /// [`Vm::define_host_module`] builds the module from them. Run it
    /// between cells; the entries live in the heap, so they survive
    /// `reset` and snapshots.
    ///
    /// # Errors
    ///
    /// A pending exception: the code's or a `MemoryError`.
    pub fn run_host_defs(
        &mut self,
        module: &str,
        prog: Option<&Arc<HostProgram>>,
        raws: &[(&str, &str)],
        msgs: &HostValue,
        data: &HostValue,
    ) -> Result<(), Raised> {
        let pg = self.prelude_globals;
        let dk = self.intern("_host_defs")?;
        let defs = if let Some(d) = self.dict_get_str(pg, dk) {
            d
        } else {
            let d = self.new_dict()?;
            self.dict_set(pg, dk, d)?;
            d
        };
        for (k, _) in self.dict_items_vec(defs) {
            let stale =
                Vm::is_str(k) && self.str_ref(k).rsplit_once('.').map_or("", |(m, _)| m) == module;
            if stale {
                self.dict_del(defs, k)?;
            }
        }
        let Some(prog) = prog else {
            return Ok(());
        };
        let mut fns = Vec::with_capacity(raws.len());
        for (call, name) in raws {
            fns.push(self.raw_hostfn(name, call)?);
        }
        let binds = [
            ("_host_raw", self.new_list(&fns)?),
            ("_host_msgs", self.py_value(msgs)?),
            ("_host_data", self.py_value(data)?),
        ];
        for (name, v) in binds {
            let k = self.intern(name)?;
            self.dict_set(pg, k, v)?;
        }
        self.prepare_cell();
        self.set_want_completion_text(false);
        // Shared code: its frames are hidden, so the session keeps no text.
        let entry = self.load_in(&prog.program, "<host>", "", crate::vm::GSLOT_PRELUDE)?;
        if let Some(s) = self
            .codes
            .get(entry as usize)
            .and_then(|c| self.sources.get_mut(c.source as usize))
        {
            s.host = Some(Arc::clone(prog));
        }
        let made = match self.run(entry) {
            Exit::Complete(v) if v.is_obj_kind(ty::DICT as u8) => v,
            Exit::Error => return Err(Raised),
            _ => return Err(self.err(T::SystemError, "host definition code did not complete")),
        };
        for (name, _) in binds {
            let k = self.intern(name)?;
            self.dict_del(pg, k)?;
        }
        for (k, v) in self.dict_items_vec(made) {
            self.dict_set(defs, k, v)?;
        }
        Ok(())
    }

    /// Every host module: its name, constants and the embedder's spec.
    #[must_use]
    #[allow(clippy::type_complexity, reason = "plain parts for the facade")]
    pub fn host_module_specs(&self) -> Vec<(String, Vec<(String, HostValue)>, HostValue)> {
        self.host_modules
            .iter()
            .map(|m| (m.name.clone(), m.constants.clone(), m.spec.clone()))
            .collect()
    }

    /// The exception host call `call`'s error raises (pending on failure);
    /// records the call as raised.
    pub(crate) fn host_exception(&mut self, e: &HostError, call: u64) -> Result<Value, Raised> {
        let function = self.settle_call(call, CallStatus::Raised);
        let m = self.new_str(&e.message)?;
        let exc = match &e.class {
            Some(name) => {
                let Some(cls) = self.host_exception_class(name) else {
                    let msg = format!("unknown host exception class '{name}'");
                    return Err(self.err(T::TypeError, &msg));
                };
                let args = self.alloc_tuple(&[m])?;
                self.alloc_exc(cls, args)?
            }
            None => self.new_exception(e.kind.class(), &[m])?,
        };
        let Some(r) = exc.as_obj() else {
            return Ok(exc);
        };
        self.set_slot(r, exc_slot::TRACEBACK, Value::NONE);
        if e.class.is_some() || e.data.is_some() {
            let tool = match function.and_then(|f| self.host_ids.get(f as usize)) {
                Some(n) => {
                    let n = n.clone();
                    self.new_str(&n)?
                }
                None => Value::NONE,
            };
            let data = match &e.data {
                Some(d) => self.py_value(d)?,
                None => Value::NONE,
            };
            let d = self.new_dict()?;
            for (k, v) in [("tool", tool), ("data", data)] {
                let k = self.intern(k)?;
                self.dict_set(d, k, v)?;
            }
            self.set_slot(r, exc_slot::DICT, d);
        }
        Ok(exc)
    }

    /// Call name `call` without its host module (`fs.Entry.read` →
    /// `Entry.read`): the function's `__qualname__`.
    pub(crate) fn host_bare_name(&self, call: &str) -> String {
        let module = self
            .host_modules
            .iter()
            .filter(|m| {
                !m.name.is_empty()
                    && call.len() > m.name.len()
                    && call.starts_with(m.name.as_str())
                    && call.as_bytes()[m.name.len()] == b'.'
            })
            .map(|m| m.name.len() + 1)
            .max()
            .unwrap_or(0);
        String::from(&call[module..])
    }

    /// The host exception class registered as `name`, if there is one.
    #[must_use]
    pub fn host_exception_class(&self, name: &str) -> Option<Value> {
        self.host_def(name)
            .filter(|&c| Vm::is_class(c) && self.class_base_t(c).is_exception())
    }

    // ------------------------------------------------------ call log

    /// Records host call `id` of function `function` as pending.
    fn log_call(&mut self, id: u64, function: u32) -> Result<(), Raised> {
        if self.call_log.len() == self.call_log.capacity() {
            let extra = self.call_log.len().max(4);
            self.reserve_execution((extra * size_of::<CallEntry>()) as u64)?;
            let grown = self.call_log.try_reserve_exact(extra);
            self.reserve_execution(0)?;
            grown.map_err(|_| self.err(T::MemoryError, "call log allocation failed"))?;
        }
        self.call_log.push((id, function, CallStatus::Pending));
        Ok(())
    }

    /// Records how host call `id` was answered; returns its function id.
    pub(crate) fn settle_call(&mut self, id: u64, status: CallStatus) -> Option<u32> {
        // Ids increase, so the log is sorted.
        let i = self.call_log.binary_search_by_key(&id, |e| e.0).ok()?;
        let entry = &mut self.call_log[i];
        if entry.2 == CallStatus::Pending {
            entry.2 = status;
        }
        Some(entry.1)
    }

    /// The host calls of the current (or last) cell, in call order:
    /// `(call id, function id, status)`. Started anew by
    /// [`Vm::clear_call_log`].
    #[must_use]
    pub fn call_log(&self) -> Vec<(u64, u32, CallStatus)> {
        self.call_log.clone()
    }

    /// Forgets the logged host calls (before a new cell starts).
    pub fn clear_call_log(&mut self) {
        self.call_log = Vec::new();
        let _ = self.reserve_execution(0);
    }

    /// Marks every unanswered host call abandoned (the cell ended).
    pub(crate) fn abandon_calls(&mut self) {
        for e in &mut self.call_log {
            if e.2 == CallStatus::Pending {
                e.2 = CallStatus::Abandoned;
            }
        }
    }
}

/// The state of one conversion: the byte budget left and why a lenient
/// conversion stopped.
/// A container [`Vm::host_value`] is filling.
enum Open {
    Seq {
        tuple: bool,
        items: alloc::vec::IntoIter<Value>,
        out: Vec<HostValue>,
        copies: usize,
    },
    Dict {
        items: alloc::vec::IntoIter<(Value, Value)>,
        out: Vec<(String, HostValue)>,
        /// The key of the item being converted.
        key: String,
        copies: usize,
    },
    Record {
        /// The named tuple (charges name it).
        x: Value,
        fields: core::iter::Zip<alloc::vec::IntoIter<String>, alloc::vec::IntoIter<Value>>,
        out: Vec<(String, HostValue)>,
        key: String,
        name: String,
        class: Option<String>,
        copies: usize,
    },
}

/// What converting one value started.
enum Opened {
    Value(HostValue),
    Open(Open),
}

impl Open {
    /// Takes the converted value of the item [`Vm::host_next`] gave.
    fn accept(&mut self, h: HostValue) {
        match self {
            Open::Seq { out, .. } => out.push(h),
            Open::Dict { out, key, .. } | Open::Record { out, key, .. } => {
                out.push((core::mem::take(key), h));
            }
        }
    }

    /// Bytes charged for the copies and stack place, released when done.
    fn copies(&self) -> usize {
        match self {
            Open::Seq { copies, .. } | Open::Dict { copies, .. } | Open::Record { copies, .. } => {
                *copies
            }
        }
    }

    fn finish(self) -> HostValue {
        match self {
            Open::Seq {
                tuple: true, out, ..
            } => HostValue::Tuple(out),
            Open::Seq { out, .. } => HostValue::List(out),
            Open::Dict { out, .. } => HostValue::Dict(out),
            Open::Record {
                out, name, class, ..
            } => match class {
                Some(class) => HostValue::Object { class, fields: out },
                None => HostValue::Record { name, fields: out },
            },
        }
    }
}

struct Conversion {
    args: HostArgs,
    budget: usize,
    stopped: Option<Unconverted>,
    /// Rust heap bytes the tree (and the item copies of the levels open)
    /// hold, reserved from the session's memory budget until the tree is
    /// handed to the host.
    held: usize,
    reserved: Option<mm_core::Scratch>,
}

impl Conversion {
    fn new(args: HostArgs) -> Self {
        Conversion {
            args,
            budget: usize::try_from(args.max_bytes).unwrap_or(usize::MAX),
            stopped: None,
            held: 0,
            reserved: None,
        }
    }

    /// Room left, in bytes, plus one (to tell "full" from "over").
    fn room(&self) -> usize {
        self.budget.saturating_add(1)
    }

    /// Stops for `reason`: a lenient conversion returns the marker, a
    /// strict one raises `raise()`.
    fn stop(
        &mut self,
        reason: Unconverted,
        raise: impl FnOnce() -> Raised,
    ) -> Result<HostValue, Raised> {
        if !self.args.lenient {
            return Err(raise());
        }
        self.stopped = Some(reason.clone());
        Ok(HostValue::Unconverted(reason))
    }
}

/// The length of `i` in decimal.
fn decimal_len(i: i64) -> usize {
    let mut n = usize::from(i < 0) + 1;
    let mut x = i.unsigned_abs();
    while x >= 10 {
        x /= 10;
        n += 1;
    }
    n
}

impl Vm {
    /// The host module and attribute of call name `call` (the module whose
    /// name is its longest prefix, else the builtins `""`).
    fn host_owner(&self, call: &str) -> Option<(Arc<HostModule>, String)> {
        let m = self
            .host_modules
            .iter()
            .filter(|m| {
                m.name.is_empty()
                    || (call.len() > m.name.len()
                        && call.starts_with(m.name.as_str())
                        && call.as_bytes()[m.name.len()] == b'.')
            })
            .max_by_key(|m| m.name.len())?;
        let attr = if m.name.is_empty() {
            call
        } else {
            &call[m.name.len() + 1..]
        };
        Some((Arc::clone(m), String::from(attr)))
    }

    /// Part `i` (0: the stub, 1: the docstring) of the help entry of
    /// attribute `attr` (`""`: the module itself) of host module `m`. The
    /// entries are the last item of the definition's `spec`: a dict from
    /// attribute to `[stub, doc]`.
    fn help_part(m: &HostModule, attr: &str, i: usize) -> Option<String> {
        let HostValue::List(spec) = &m.spec else {
            return None;
        };
        let Some(HostValue::Dict(entries)) = spec.last() else {
            return None;
        };
        let (_, HostValue::List(parts)) = entries.iter().find(|(k, _)| k == attr)? else {
            return None;
        };
        match parts.get(i) {
            Some(HostValue::Str(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        }
    }

    /// `__doc__` (`i == 1`) or the stub (`i == 0`) of host function `f`.
    pub(crate) fn hostfn_help(&self, f: Value, i: usize) -> Option<String> {
        let call = self.str_ref(self.vslot(f, hostfn_slot::CALL_NAME));
        let (m, attr) = self.host_owner(call)?;
        Vm::help_part(&m, &attr, i)
    }

    /// `__module__` of host function `f`: its host module, or `builtins`.
    pub(crate) fn hostfn_module(&self, f: Value) -> String {
        let call = self.str_ref(self.vslot(f, hostfn_slot::CALL_NAME));
        match self.host_owner(call) {
            Some((m, _)) if !m.name.is_empty() => m.name.clone(),
            _ => String::from("builtins"),
        }
    }
}

/// `_mm.host_help(obj)`: the stub of a host function, class, exception or
/// module, or `None` for anything else (`help()` prints it).
pub(crate) fn mm_host_help(vm: &mut Vm, a: crate::vm::CallArgs) -> Result<Value, Raised> {
    let obj = vm.arg(&a, 0);
    let text = if obj.is_obj_kind(ty::HOSTFN as u8) {
        vm.hostfn_help(obj, 0)
    } else if obj.is_obj_kind(ty::MODULE as u8) {
        let name = vm.module_name(obj);
        vm.host_modules
            .iter()
            .find(|m| m.name == name)
            .and_then(|m| Vm::help_part(m, "", 0))
    } else {
        // A typed function or a class the host defined: its key in
        // `_host_defs`.
        let defs = vm
            .find_interned("_host_defs")
            .and_then(|k| vm.dict_get_str(vm.prelude_globals, k));
        let key = defs.and_then(|d| {
            vm.dict_items_vec(d)
                .into_iter()
                .find(|&(_, v)| v == obj)
                .map(|(k, _)| k)
        });
        key.filter(|&k| Vm::is_str(k)).and_then(|k| {
            let (m, attr) = vm.host_owner(vm.str_ref(k))?;
            Vm::help_part(&m, &attr, 0)
        })
    };
    match text {
        Some(t) => vm.new_str(&t),
        None => Ok(Value::NONE),
    }
}

/// Why `namedtuple(name, fields)` would refuse these names (CPython's
/// messages, without `rename`): each an identifier and not a keyword, a
/// field not starting with `_` and not repeated.
fn record_names_error(name: &str, fields: &[&str]) -> Option<String> {
    let keyword = |s: &str| {
        matches!(
            s,
            "False"
                | "None"
                | "True"
                | "and"
                | "as"
                | "assert"
                | "async"
                | "await"
                | "break"
                | "class"
                | "continue"
                | "def"
                | "del"
                | "elif"
                | "else"
                | "except"
                | "finally"
                | "for"
                | "from"
                | "global"
                | "if"
                | "import"
                | "in"
                | "is"
                | "lambda"
                | "nonlocal"
                | "not"
                | "or"
                | "pass"
                | "raise"
                | "return"
                | "try"
                | "while"
                | "with"
                | "yield"
        )
    };
    let repr = crate::text::str_repr;
    for (k, n) in core::iter::once(&name).chain(fields).enumerate() {
        if !crate::strmeth::is_identifier(n) {
            return Some(format!(
                "Type names and field names must be valid identifiers: {}",
                repr(n)
            ));
        }
        if keyword(n) {
            return Some(format!(
                "Type names and field names cannot be a keyword: {}",
                repr(n)
            ));
        }
        if k > 0 && n.starts_with('_') {
            return Some(format!(
                "Field names cannot start with an underscore: {}",
                repr(n)
            ));
        }
        if k > 0 && fields[..k - 1].contains(n) {
            return Some(format!("Encountered duplicate field name: {}", repr(n)));
        }
    }
    None
}
