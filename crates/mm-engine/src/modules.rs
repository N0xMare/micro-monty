//! Modules: the bundled native ones (`asyncio`, `json`, `math`, `sys`,
//! `_mm`), the bundled Python modules (compiled at build time, loaded and
//! run on first import), host modules registered by the embedder, and
//! `import`. Nothing else is importable (docs/spec.md §4, §12).
//!
//! A bundled Python module's body runs as a frame of the importing
//! dispatch loop: `import` leaves a call request whose continuation
//! (`Cont::Import`) registers the module and goes on with the import.

use crate::cont::Cont;
use crate::types::{T, ty};
use crate::vm::{GSLOT_STD, Raised, Request, Stream, Vm};
use alloc::{format, string::String, sync::Arc, vec::Vec};
use mm_core::Value;

/// Module slots.
pub(crate) mod module_slot {
    pub const NAME: u32 = 0;
    pub const DICT: u32 = 1;
}

/// The `asyncio` names written in the prelude (docs/spec.md §8.5).
#[cfg(feature = "asyncio")]
pub(crate) const ASYNCIO_PRELUDE: [&str; 38] = [
    "ALL_COMPLETED",
    "Barrier",
    "BoundedSemaphore",
    "BrokenBarrierError",
    "Condition",
    "Event",
    "EventLoop",
    "FIRST_COMPLETED",
    "FIRST_EXCEPTION",
    "Future",
    "Handle",
    "LifoQueue",
    "Lock",
    "PriorityQueue",
    "Queue",
    "QueueEmpty",
    "QueueFull",
    "QueueShutDown",
    "Runner",
    "Semaphore",
    "TaskGroup",
    "Timeout",
    "TimerHandle",
    "all_tasks",
    "as_completed",
    "current_task",
    "ensure_future",
    "get_event_loop",
    "get_running_loop",
    "isfuture",
    "new_event_loop",
    "run",
    "set_event_loop",
    "shield",
    "timeout",
    "timeout_at",
    "wait",
    "wait_for",
];

/// `asyncio`'s submodules, each a view of `asyncio`'s own objects.
#[cfg(feature = "asyncio")]
const ASYNCIO_SUBMODULES: [(&str, &[&str]); 9] = [
    (
        "events",
        &[
            "Handle",
            "TimerHandle",
            "get_event_loop",
            "get_running_loop",
            "new_event_loop",
            "set_event_loop",
            "_set_event_loop_policy",
        ],
    ),
    (
        "exceptions",
        &[
            "BrokenBarrierError",
            "CancelledError",
            "InvalidStateError",
            "TimeoutError",
        ],
    ),
    ("futures", &["Future", "isfuture"]),
    (
        "locks",
        &[
            "Lock",
            "Event",
            "Condition",
            "Semaphore",
            "BoundedSemaphore",
            "Barrier",
        ],
    ),
    (
        "queues",
        &[
            "Queue",
            "PriorityQueue",
            "LifoQueue",
            "QueueFull",
            "QueueEmpty",
            "QueueShutDown",
        ],
    ),
    ("runners", &["Runner", "run"]),
    ("taskgroups", &["TaskGroup"]),
    (
        "tasks",
        &[
            "Task",
            "create_task",
            "FIRST_COMPLETED",
            "FIRST_EXCEPTION",
            "ALL_COMPLETED",
            "wait",
            "wait_for",
            "as_completed",
            "sleep",
            "gather",
            "shield",
            "ensure_future",
            "current_task",
            "all_tasks",
        ],
    ),
    ("timeouts", &["Timeout", "timeout", "timeout_at"]),
];

/// A package's submodule: its name and the parent's names it holds.
type Submodule = (&'static str, &'static [&'static str]);

/// `json`'s submodules and the names each holds, as for asyncio.
#[cfg(feature = "json")]
const JSON_SUBMODULES: [(&str, &[&str]); 2] = [
    ("decoder", &["JSONDecoder", "JSONDecodeError"]),
    ("encoder", &["JSONEncoder"]),
];

/// Bundled native modules (`_mm` is the engine's own); `asyncio` and
/// `json` with their features.
pub const BUNDLED: &[&str] = &[
    "_mm",
    #[cfg(feature = "asyncio")]
    "asyncio",
    "cmath",
    #[cfg(feature = "json")]
    "json",
    "math",
    "sys",
];

/// Whether `import name` finds an engine module ([`BUNDLED`], or a
/// submodule of a bundled package) before any host module of that name.
#[must_use]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn is_bundled(name: &str) -> bool {
    let packages: &[(&str, &[Submodule])] = &[
        #[cfg(feature = "asyncio")]
        ("asyncio", &ASYNCIO_SUBMODULES),
        #[cfg(feature = "json")]
        ("json", &JSON_SUBMODULES),
    ];
    BUNDLED.contains(&name)
        || packages.iter().any(|(p, subs)| {
            name.strip_prefix(p)
                .and_then(|s| s.strip_prefix('.'))
                .is_some_and(|s| subs.iter().any(|(n, _)| *n == s))
        })
}

impl Vm {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn new_module(&mut self, name: &str) -> Result<Value, Raised> {
        let d = self.new_dict()?;
        self.new_module_with(name, d)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn new_module_with(&mut self, name: &str, d: Value) -> Result<Value, Raised> {
        let n = self.intern(name)?;
        let r = {
            let res = self.heap.alloc_init(ty::MODULE, 0, &[n, d], &[]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::MODULE as u8))
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn module_set(&mut self, m: Value, name: &str, v: Value) -> Result<(), Raised> {
        let k = self.intern(name)?;
        let d = self.vslot(m, module_slot::DICT);
        self.dict_set(d, k, v)
    }

    /// The module's name.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn module_name(&self, m: Value) -> String {
        String::from(self.str_ref(self.vslot(m, module_slot::NAME)))
    }

    /// `module.name`, if it has it.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn module_attr(&mut self, m: Value, name: Value) -> Result<Option<Value>, Raised> {
        if self.str_ref(name) == "__name__" {
            return Ok(Some(self.vslot(m, module_slot::NAME)));
        }
        let d = self.vslot(m, module_slot::DICT);
        if self.str_ref(name) == "__dict__" {
            self.module_build_all(d)?;
            return Ok(Some(d));
        }
        match self.dict_get(d, name)? {
            Some(v) if v == Value::UNDEFINED => self.module_build(d, name).map(Some),
            v => Ok(v),
        }
    }

    /// Binds `attrs` in module `m` as lazy prelude units: each value is
    /// `UNDEFINED` (which no Python code can hold) until the first lookup
    /// builds the prelude global of the same name. The dict keeps its key
    /// order, and a snapshot keeps which names are still unbuilt.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn module_set_lazy(&mut self, m: Value, attrs: &[&str]) -> Result<(), Raised> {
        for attr in attrs {
            if crate::prelude::has_unit(attr) {
                self.module_set(m, attr, Value::UNDEFINED)?;
            }
        }
        Ok(())
    }

    /// Module dict `d`'s lazy entry `name`, built now and bound.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn module_build(&mut self, d: Value, name: Value) -> Result<Value, Raised> {
        let Some(v) = self.prelude_get(name)? else {
            return Err(self.err(T::SystemError, "micro-monty: the prelude is corrupt"));
        };
        self.dict_set(d, name, v)?;
        Ok(v)
    }

    /// Builds every lazy entry of module dict `d` (before the dict itself
    /// is handed out), in one batch.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn module_build_all(&mut self, d: Value) -> Result<(), Raised> {
        let lazy: Vec<String> = self
            .dict_items_vec(d)
            .into_iter()
            .filter(|&(_, v)| v == Value::UNDEFINED)
            .map(|(k, _)| String::from(self.str_ref(k)))
            .collect();
        if lazy.is_empty() {
            return Ok(());
        }
        let names: Vec<&str> = lazy.iter().map(String::as_str).collect();
        for (n, v) in names.iter().zip(self.prelude_get_all(&names)?) {
            let Some(v) = v else {
                return Err(self.err(T::SystemError, "micro-monty: the prelude is corrupt"));
            };
            let k = self.intern(n)?;
            self.dict_set(d, k, v)?;
        }
        Ok(())
    }

    /// `module.name = v`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn module_setattr(&mut self, m: Value, name: Value, v: Value) -> Result<(), Raised> {
        let d = self.vslot(m, module_slot::DICT);
        self.dict_set(d, name, v)
    }

    /// The module named `dotted` (imported, or built now), if it exists.
    /// `UNDEFINED` with a call request for a bundled Python module whose
    /// body must run first (`dotted` is the whole import, `top` and `from_`
    /// as in [`Cont::Import`]).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn find_module(
        &mut self,
        dotted: &str,
        whole: Value,
        top: bool,
        from_: bool,
    ) -> Result<Option<Value>, Raised> {
        let k = self.intern(dotted)?;
        if let Some(m) = self.dict_get(self.modules, k)? {
            return Ok(Some(m));
        }
        if let Some(i) = self.stdlib.iter().position(|m| m.name == dotted) {
            return self.start_std(i, whole, top, from_).map(Some);
        }
        let packages: &[(&str, &[Submodule])] = &[
            #[cfg(feature = "asyncio")]
            ("asyncio", &ASYNCIO_SUBMODULES),
            #[cfg(feature = "json")]
            ("json", &JSON_SUBMODULES),
        ];
        let sub = packages.iter().find_map(|(p, subs)| {
            let s = dotted.strip_prefix(p)?.strip_prefix('.')?;
            subs.iter()
                .find(|(n, _)| *n == s)
                .map(|(n, a)| (*p, *n, *a))
        });
        let m = if BUNDLED.contains(&dotted) {
            self.build_bundled(dotted)?
        } else if let Some((pname, sub, names)) = sub {
            let Some(parent) = self.find_module(pname, Value::NONE, false, true)? else {
                return Ok(None);
            };
            let m = self.new_module(dotted)?;
            for attr in names {
                let a = self.intern(attr)?;
                let v = match self.module_attr(parent, a)? {
                    Some(v) => Some(v),
                    None => self.prelude_get(a)?,
                };
                if let Some(v) = v {
                    self.module_set(m, attr, v)?;
                }
            }
            self.module_set(parent, sub, m)?;
            m
        } else if let Some(i) = self.host_modules.iter().position(|m| m.name == dotted) {
            self.build_host(i)?
        } else {
            return Ok(None);
        };
        let mods = self.modules;
        self.dict_set(mods, k, m)?;
        // CPython's `json` imports its submodules, so `json.decoder` is an
        // attribute after `import json` (`except json.decoder.JSONDecodeError`).
        #[cfg(feature = "json")]
        if dotted == "json" {
            for (sub, _) in JSON_SUBMODULES {
                self.find_module(&format!("json.{sub}"), Value::NONE, false, true)?;
            }
        }
        Ok(Some(m))
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn build_bundled(&mut self, name: &str) -> Result<Value, Raised> {
        let m = self.new_module(name)?;
        for (i, n) in crate::builtins::NATIVES.iter().enumerate() {
            if n.module() == Some(name) {
                let f = Value::native(u32::try_from(i).unwrap_or(0));
                self.module_set(m, n.name(), f)?;
            }
        }
        #[cfg(feature = "json")]
        if name == "json" {
            // Written in the prelude (docs/spec.md §12).
            let attrs = [
                "dumps",
                "loads",
                "dump",
                "load",
                "JSONDecodeError",
                "JSONDecoder",
                "JSONEncoder",
            ];
            for (attr, v) in attrs.iter().zip(self.prelude_get_all(&attrs)?) {
                if let Some(v) = v {
                    self.module_set(m, attr, v)?;
                }
            }
        }
        #[cfg(feature = "asyncio")]
        if name == "asyncio" {
            // Written in the prelude (docs/spec.md §8.5), built on first use.
            self.module_set_lazy(m, &ASYNCIO_PRELUDE)?;
            for t in [
                T::CancelledError,
                T::InvalidStateError,
                T::TimeoutError,
                T::Task,
            ] {
                let v = self.types[t as usize];
                self.module_set(m, t.name(), v)?;
            }
        }
        if name == "cmath" {
            self.cmath_constants(m)?;
        }
        if name == "math" {
            for (attr, x) in crate::math::CONSTANTS {
                self.module_set(m, attr, Value::float(x))?;
            }
        }
        if name == "sys" {
            for (attr, text) in [
                ("platform", "linux"),
                ("version", "3.14.7 (micro-monty)"),
                ("byteorder", "little"),
                ("float_repr_style", "short"),
            ] {
                let v = self.new_str(text)?;
                self.module_set(m, attr, v)?;
            }
            for (attr, i) in [
                ("maxsize", i64::MAX),
                ("maxunicode", 0x0010_FFFF),
                ("hexversion", 0x030E_07F0),
                ("api_version", 1013),
            ] {
                let v = self.new_int(i)?;
                self.module_set(m, attr, v)?;
            }
            let attrs = [
                "version_info",
                "float_info",
                "hash_info",
                "flags",
                "int_info",
                "get_int_max_str_digits",
                "set_int_max_str_digits",
            ];
            let units = [
                "_version_info",
                "_float_info",
                "_hash_info",
                "_flags",
                "_int_info",
                "get_int_max_str_digits",
                "set_int_max_str_digits",
            ];
            for (attr, v) in attrs.iter().zip(self.prelude_get_all(&units)?) {
                if let Some(v) = v {
                    self.module_set(m, attr, v)?;
                }
            }
            let copyright = self.new_str("Copyright (c) 2001 Python Software Foundation.")?;
            self.module_set(m, "copyright", copyright)?;
            self.module_set(m, "pycache_prefix", Value::NONE)?;
            let mut names = Vec::new();
            for &n in BUNDLED {
                names.push(self.new_str(n)?);
            }
            let names = self.alloc_tuple(&names)?;
            self.module_set(m, "builtin_module_names", names)?;
            let script = self.new_str("")?;
            let argv = self.new_list(&[script])?;
            self.module_set(m, "argv", argv)?;
            // Imports never search a path (docs/user-guide.md "Known
            // differences from CPython"): an empty list that code may read
            // and edit without effect.
            let path = self.new_list(&[])?;
            self.module_set(m, "path", path)?;
            for (attr, id) in [("stdout", 1), ("stderr", 2)] {
                let r = {
                    let res = self.heap.alloc_init(ty::STREAM, 0, &[], &[id]);
                    self.hx(res)
                }?;
                self.module_set(m, attr, Value::from_ref(r, ty::STREAM as u8))?;
            }
        }
        Ok(m)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn build_host(&mut self, i: usize) -> Result<Value, Raised> {
        let module = Arc::clone(&self.host_modules[i]);
        let m = self.new_module(&module.name)?;
        for f in &module.functions {
            let v = self.new_hostfn(f, &format!("{}.{f}", module.name))?;
            self.module_set(m, f, v)?;
        }
        for (k, h) in &module.constants {
            let v = self.py_value(h)?;
            self.module_set(m, k, v)?;
        }
        // Submodules are attributes of their parent.
        for i in 0..self.host_modules.len() {
            let child = Arc::clone(&self.host_modules[i]);
            let Some(attr) = child
                .name
                .strip_prefix(&module.name)
                .and_then(|rest| rest.strip_prefix('.'))
                .filter(|rest| !rest.contains('.'))
            else {
                continue;
            };
            if let Some(c) = self.find_module(&child.name, Value::NONE, false, true)? {
                self.module_set(m, attr, c)?;
            }
        }
        Ok(m)
    }

    /// Whether module `name` is a package: one of CPython's that mm bundles
    /// submodules of, or a host module (host modules are packages).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn is_package(&self, name: &str) -> bool {
        matches!(name, "collections" | "json" | "asyncio" | "urllib" | "html")
            || self.host_modules.iter().any(|h| h.name == name)
    }

    /// `import dotted`: the leaf module, or the top-level one if `top`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn import(&mut self, dotted: Value, top: bool) -> Result<Value, Raised> {
        let name = String::from(self.str_ref(dotted));
        if name.starts_with('.') {
            return Err(self.err(
                T::ImportError,
                "attempted relative import with no known parent package",
            ));
        }
        let mut first = Value::NONE;
        let mut last = Value::NONE;
        let mut prefix = String::new();
        for (k, part) in name.split('.').enumerate() {
            if k > 0 {
                prefix.push('.');
            }
            prefix.push_str(part);
            let Some(m) = self.find_module(&prefix, dotted, top, false)? else {
                let mut msg = format!("No module named '{prefix}'");
                // Name the parent when CPython's is a plain module, not a
                // package (host modules are packages).
                let parent = self.module_name(last);
                if k > 0 && !self.is_package(&parent) {
                    msg = format!("{msg}; '{parent}' is not a package");
                }
                let e = self.err(T::ModuleNotFoundError, &msg);
                self.set_import_names(&prefix, Value::UNDEFINED);
                self.import_hint(&prefix);
                return Err(e);
            };
            if m == Value::UNDEFINED {
                return Ok(m);
            }
            if k == 0 {
                first = m;
            }
            last = m;
        }
        Ok(if top { first } else { last })
    }

    /// `from module import name`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn import_from(&mut self, m: Value, name: Value, g: Value) -> Result<Value, Raised> {
        if self.str_ref(name) == "*" {
            return self.import_star(m, g);
        }
        if let Some(v) = self.module_attr(m, name)? {
            return Ok(v);
        }
        let mname = self.module_name(m);
        let n = String::from(self.str_ref(name));
        if let Some(sub) = self.find_module(&format!("{mname}.{n}"), Value::NONE, false, true)? {
            return Ok(sub);
        }
        let e = self.err(
            T::ImportError,
            &format!("cannot import name '{n}' from '{mname}' (unknown location)"),
        );
        self.set_import_names(&mname, name);
        self.module_attr_hint(m, name, true);
        Err(e)
    }

    /// `from m import *` into globals `g`: the names in `m.__all__`, else
    /// the ones not starting with `_`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn import_star(&mut self, m: Value, g: Value) -> Result<Value, Raised> {
        let d = self.vslot(m, module_slot::DICT);
        let k = self.intern("__all__")?;
        let names = match self.dict_get(d, k)? {
            Some(all) => {
                let Some(names) = self.seq_items(all) else {
                    let msg = format!(
                        "micro-monty does not support a {} as __all__",
                        self.type_name(all)
                    );
                    return Err(self.err(T::TypeError, &msg));
                };
                for &n in &names {
                    if !Vm::is_str(n) {
                        let msg = format!(
                            "Item in {}.__all__ must be str, not {}",
                            self.module_name(m),
                            self.type_name(n)
                        );
                        return Err(self.err(T::TypeError, &msg));
                    }
                }
                names
            }
            None => self
                .dict_items_vec(d)
                .into_iter()
                .map(|(n, _)| n)
                .filter(|&n| Vm::is_str(n) && !self.str_ref(n).starts_with('_'))
                .collect(),
        };
        self.burn(names.len() as u64);
        for n in names {
            let Some(v) = self.module_attr(m, n)? else {
                let msg = format!(
                    "module '{}' has no attribute '{}'",
                    self.module_name(m),
                    self.str_ref(n)
                );
                return Err(self.err(T::AttributeError, &msg));
            };
            self.dict_set(g, n, v)?;
        }
        Ok(Value::NONE)
    }

    /// Starts importing bundled Python module `i`: loads its code (once
    /// per session), makes its module and globals, and leaves a request to
    /// run its body, finished by [`Cont::Import`].
    #[cfg_attr(mm_shipped, optimize(size))]
    fn start_std(
        &mut self,
        i: usize,
        whole: Value,
        top: bool,
        from_: bool,
    ) -> Result<Value, Raised> {
        let m = self.stdlib[i];
        let gslot = GSLOT_STD + u32::try_from(i).unwrap_or(0);
        let entry = if let Some((_, _, entry)) = self.std_codes[i] {
            entry
        } else {
            // Previously reserved workspace can be reused. No arena or other
            // VM allocation occurs while these bounded decoders run.
            let allowance = self.heap.available_bytes().saturating_add(self.std_scratch);
            let mut remaining = allowance;
            let prog = m
                .decode(
                    i,
                    self.module_cache.as_deref(),
                    &mut self.unpackers,
                    &mut remaining,
                )
                .map_err(|e| match e {
                    mm_compile::DecodeError::TooLarge => self.err(T::MemoryError, ""),
                    mm_compile::DecodeError::Invalid => {
                        self.err(T::SystemError, "micro-monty: a bundled module is corrupt")
                    }
                })?;
            let old_scratch = self.std_scratch;
            self.std_scratch = old_scratch.max(allowance - remaining);
            let first = u32::try_from(self.codes.len()).unwrap_or(u32::MAX);
            let loaded = self.load_in(&prog.program, &format!("<{}>", m.name), "", gslot);
            if loaded.is_err() {
                self.std_scratch = old_scratch;
                self.reserve_code(0)?;
            }
            let entry = loaded?;
            let end = u32::try_from(self.codes.len()).unwrap_or(u32::MAX);
            self.std_codes[i] = Some((first, end, entry));
            entry
        };
        let g = self.new_globals_dict()?;
        let module = self.new_module_with(m.name, g)?;
        let k = self.intern("__name__")?;
        let n = self.vslot(module, module_slot::NAME);
        self.dict_set(g, k, n)?;
        // Native parts of the module come first; its Python code may wrap
        // or replace them.
        for (j, nat) in crate::builtins::NATIVES.iter().enumerate() {
            if nat.module() == Some(m.name) {
                let f = Value::native(u32::try_from(j).unwrap_or(0));
                self.module_set(module, nat.name(), f)?;
            }
        }
        // And its builtin type.
        #[cfg(feature = "re")]
        if m.name == "re" {
            let v = self.types[T::Match as usize];
            self.module_set(module, "Match", v)?;
        }
        self.std_globals[i] = g;
        // In `sys.modules` while its body runs, so a circular import (a
        // package's submodule importing the package) finds it.
        let mods = self.modules;
        self.dict_set(mods, n, module)?;
        let name = self.intern("<module>")?;
        let f = {
            let res = self.heap.alloc_init(
                ty::FUNC,
                0,
                &[
                    name,
                    name,
                    Value::NONE,
                    Value::NONE,
                    Value::NONE,
                    g,
                    Value::NONE,
                ],
                &[u64::from(entry)],
            );
            self.hx(res)
        }?;
        let dotted = if whole == Value::NONE { n } else { whole };
        self.request = Some(Request::Call {
            func: Value::from_ref(f, ty::FUNC as u8),
            args: crate::cont::Args::of(&[]),
            kw: Value::NONE,
            cont: Cont::Import {
                module,
                dotted,
                top,
                from_,
            },
            outer: Cont::None,
        });
        Ok(Value::UNDEFINED)
    }

    /// A bundled module's body returned: `sys.modules` gets it, and its
    /// package an attribute for it.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn imported(&mut self, module: Value) -> Result<(), Raised> {
        let name = self.module_name(module);
        let k = self.vslot(module, module_slot::NAME);
        let mods = self.modules;
        self.dict_set(mods, k, module)?;
        if let Some((parent, leaf)) = name.rsplit_once('.') {
            let pk = self.intern(parent)?;
            if let Some(p) = self.dict_get(self.modules, pk)? {
                self.module_set(p, leaf, module)?;
            }
        }
        Ok(())
    }

    /// `sys.<name>` (`stdout` or `stderr`) when the program replaced it
    /// with something other than one of the engine's streams
    /// (`contextlib.redirect_stdout`).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn replaced_stream(&mut self, name: &str) -> Result<Option<Value>, Raised> {
        let k = self.intern("sys")?;
        let Some(sys) = self.dict_get_str(self.modules, k) else {
            return Ok(None);
        };
        let k = self.intern(name)?;
        let d = self.vslot(sys, module_slot::DICT);
        Ok(self
            .dict_get_str(d, k)
            .filter(|v| !v.is_obj_kind(ty::STREAM as u8)))
    }

    /// The stream a `sys.stdout`/`sys.stderr` object writes to.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn stream_of(&self, v: Value) -> Option<Stream> {
        if !v.is_obj_kind(ty::STREAM as u8) {
            return None;
        }
        match v.as_obj().map(|r| self.raw(r, 0)) {
            Some(2) => Some(Stream::Stderr),
            _ => Some(Stream::Stdout),
        }
    }
}
