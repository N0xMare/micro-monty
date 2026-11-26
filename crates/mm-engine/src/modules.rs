//! Modules: the bundled native ones (`asyncio`, `json`, `math`, `sys`,
//! `_mm`), the bundled Python modules (compiled at build time, loaded and
//! run on first import), host modules registered by the embedder, and
//! `import`. Nothing else is importable (docs/spec.md §4, §12).
//!
//! A bundled Python module's body runs as a frame of the importing
//! dispatch loop: `import` leaves a call request whose continuation
//! (`Cont::Import`) registers the module and goes on with the import.

use crate::builtins::{
    NATIVES,
    module_specs::{self as spec, Binding},
};
use crate::cont::Cont;
use crate::types::{T, ty};
use crate::vm::{GSLOT_STD, Raised, Request, Stream, Vm};
use alloc::{format, string::String, sync::Arc, vec::Vec};
use mm_core::Value;

pub use spec::BUNDLED;

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

/// Whether `import name` finds an engine module ([`BUNDLED`], or a
/// submodule of a bundled package) before any host module of that name.
#[must_use]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn is_bundled(name: &str) -> bool {
    spec::find(name).is_some()
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
        self.alloc_module(n, d)
    }

    fn alloc_module(&mut self, n: Value, d: Value) -> Result<Value, Raised> {
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
        String::from(self.str_ref_hot(self.vslot_hot(m, module_slot::NAME)))
    }

    /// `module.name`, if it has it.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn module_attr(&mut self, m: Value, name: Value) -> Result<Option<Value>, Raised> {
        if self.str_ref_hot(name) == "__name__" {
            return Ok(Some(self.vslot_hot(m, module_slot::NAME)));
        }
        let d = self.vslot_hot(m, module_slot::DICT);
        if self.str_ref_hot(name) == "__dict__" {
            self.module_build_all(m, d)?;
            return Ok(Some(d));
        }
        match self.dict_get(d, name)? {
            Some(v) if v == Value::UNDEFINED => self.module_build(d, name).map(Some),
            v => Ok(v),
        }
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

    /// Builds all remaining lazy exports in one batch before handing out
    /// the dict. The recipe bounds the lookup work even after guest writes.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn module_build_all(&mut self, m: Value, d: Value) -> Result<(), Raised> {
        let Some(s) = spec::find(self.str_ref(self.vslot(m, module_slot::NAME))) else {
            return Ok(());
        };
        let mut names = Vec::new();
        for entry in s.entries() {
            if matches!(entry.binding, Binding::Lazy | Binding::Copy(_)) {
                let name = entry.name();
                if let Some(k) = self.find_interned(name)
                    && self.dict_get_str(d, k) == Some(Value::UNDEFINED)
                {
                    names.push(name);
                }
            }
        }
        if names.is_empty() {
            return Ok(());
        }
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
        let d = self.vslot_hot(m, module_slot::DICT);
        self.dict_set(d, name, v)
    }

    /// Deletes an attribute, including an unbuilt lazy export.
    pub(crate) fn module_delattr(&mut self, m: Value, name: Value) -> Result<(), Raised> {
        let d = self.vslot(m, module_slot::DICT);
        if self.dict_del(d, name)?.is_none() {
            return Err(self.module_attr_error(m, name));
        }
        Ok(())
    }

    pub(crate) fn module_attr_error(&mut self, m: Value, name: Value) -> Raised {
        let module = self.vslot(m, module_slot::NAME);
        let msg = format!(
            "module '{}' has no attribute '{}'",
            self.str_ref(module),
            self.str_ref(name)
        );
        self.err(T::AttributeError, &msg)
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
        let mods = self.modules;
        if let Some(s) = spec::find(dotted) {
            // A dotted import visits the parent first; a bundled child
            // can only be found in the registry, never built here.
            if dotted.contains('.') {
                return Ok(None);
            }
            let mut children = [Value::NONE; spec::CHILD_COUNT + 1];
            let m = self.build_bundled(s, k, &[], &mut children)?;
            // The first empty slot stands for the parent, published last.
            for (added, &child) in children.iter().enumerate() {
                let last = child == Value::NONE;
                let module = if last { m } else { child };
                let key = self.vslot(module, module_slot::NAME);
                if let Err(error) = self.dict_set(mods, key, module) {
                    for &child in &children[..added] {
                        let key = self.vslot(child, module_slot::NAME);
                        let _ = self.dict_del(mods, key);
                    }
                    return Err(error);
                }
                if last {
                    break;
                }
            }
            return Ok(Some(m));
        }
        let Some(i) = self.host_modules.iter().position(|m| m.name == dotted) else {
            return Ok(None);
        };
        let m = self.build_host(i)?;
        self.dict_set(mods, k, m)?;
        Ok(Some(m))
    }

    /// Fills an ordinary dict at its final size. A child can only copy an
    /// already filled entry of its parent; the const table bounds the depth.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn build_bundled(
        &mut self,
        s: spec::Spec,
        name: Value,
        parent_entries: &[[Value; 2]],
        children: &mut [Value],
    ) -> Result<Value, Raised> {
        let natives = s.natives();
        let entries = s.entries();
        self.burn((natives.len() + entries.len()) as u64);
        let mut pairs = [[Value::NONE; 2]; spec::MAX_ENTRIES];
        let mut used = natives.len();
        for (pair, i) in pairs.iter_mut().zip(natives) {
            *pair = [
                self.intern(NATIVES[i].name())?,
                Value::native(u32::try_from(i).unwrap_or(0)),
            ];
        }
        let mut eager = Vec::new();
        let mut next_child = 0;
        for entry in entries {
            let [k, v] = if let Binding::Copy(i) = entry.binding {
                parent_entries[usize::from(i)]
            } else {
                let k = self.intern(entry.name())?;
                let v = match entry.binding {
                    Binding::Lazy => Value::UNDEFINED,
                    Binding::Eager(i) => {
                        if i == 0 {
                            eager = self.prelude_get_all(spec::eager_units(s))?;
                        }
                        eager[usize::from(i)].ok_or_else(|| {
                            self.err(T::SystemError, "micro-monty: the prelude is corrupt")
                        })?
                    }
                    Binding::Type(t) => self.types[t as usize],
                    Binding::Float(i) => Value::float(crate::math::CONSTANTS[usize::from(i)].1),
                    Binding::Imaginary(i) => {
                        self.new_complex(0.0, crate::math::CONSTANTS[usize::from(i)].1)?
                    }
                    Binding::Text(i) => self.new_str(spec::text(i))?,
                    Binding::Int(i) => self.new_int(spec::integer(i))?,
                    Binding::None => Value::NONE,
                    Binding::BuiltinNames => {
                        let mut names = [Value::NONE; BUNDLED.len() - 1];
                        for (&n, v) in BUNDLED[1..].iter().zip(&mut names) {
                            *v = self.new_str(n)?;
                        }
                        self.alloc_tuple(&names)?
                    }
                    Binding::Argv => {
                        let script = self.new_str("")?;
                        self.new_list(&[script])?
                    }
                    Binding::Path => self.new_list(&[])?,
                    Binding::Stream(id) => {
                        let res = self.heap.alloc_init(ty::STREAM, 0, &[], &[u64::from(id)]);
                        Value::from_ref(self.hx(res)?, ty::STREAM as u8)
                    }
                    Binding::Child(i) => {
                        let child = spec::SPECS[usize::from(i)];
                        let n = self.intern(child.name())?;
                        let v = self.build_bundled(child, n, &pairs[..used], &mut [])?;
                        children[next_child] = v;
                        next_child += 1;
                        v
                    }
                    Binding::All(len) => {
                        let len = usize::from(len);
                        let mut names = [Value::NONE; spec::MAX_ENTRIES];
                        for (name, &[key, _]) in names.iter_mut().zip(&pairs[..len]) {
                            *name = key;
                        }
                        self.alloc_tuple(&names[..len])?
                    }
                    Binding::Copy(_) => unreachable!(),
                };
                [k, v]
            };
            pairs[used] = [k, v];
            used += 1;
        }
        let d = self.dict_from_namespace(&pairs[..used])?;
        self.alloc_module(name, d)
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

    /// [`Vm::import`] for code that runs in user globals (`user`) or for
    /// `__import__`: `_mm`, the bundled modules' engine helpers, is not
    /// found there, as if it did not exist.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn import_checked(
        &mut self,
        dotted: Value,
        top: bool,
        user: bool,
    ) -> Result<Value, Raised> {
        if user && self.str_ref(dotted).split('.').next() == Some("_mm") {
            let e = self.err(T::ModuleNotFoundError, "No module named '_mm'");
            self.set_import_names("_mm", Value::UNDEFINED);
            return Err(e);
        }
        self.import(dotted, top)
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
        if self.str_ref_hot(name) == "*" {
            return self.import_star(m, g);
        }
        if let Some(v) = self.module_attr(m, name)? {
            return Ok(v);
        }
        let mname = self.module_name(m);
        let n = String::from(self.str_ref_hot(name));
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
        let d = self.vslot_hot(m, module_slot::DICT);
        let k = self.intern_hot("__all__")?;
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
                .filter(|&n| Vm::is_str(n) && !self.str_ref_hot(n).starts_with('_'))
                .collect(),
        };
        self.burn(names.len() as u64);
        for n in names {
            let Some(v) = self.module_attr(m, n)? else {
                return Err(self.module_attr_error(m, n));
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
                    false,
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
        let g = self.new_globals_dict(8)?;
        let module = self.new_module_with(m.name, g)?;
        let k = self.intern("__name__")?;
        let n = self.vslot(module, module_slot::NAME);
        self.dict_set(g, k, n)?;
        // Native parts of the module come first; its Python code may wrap
        // or replace them.
        for (j, nat) in NATIVES.iter().enumerate() {
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
        let k = self.vslot_hot(module, module_slot::NAME);
        let mods = self.modules;
        self.dict_set(mods, k, module)?;
        if let Some((parent, leaf)) = name.rsplit_once('.') {
            let pk = self.intern_hot(parent)?;
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
        let k = self.intern_hot("sys")?;
        let Some(sys) = self.dict_get_str(self.modules, k) else {
            return Ok(None);
        };
        let k = self.intern_hot(name)?;
        let d = self.vslot_hot(sys, module_slot::DICT);
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
