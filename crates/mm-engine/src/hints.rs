//! Redirect hints: a sentence after CPython's message for code that reaches
//! past the sandbox, such as `import numpy` or `asyncio.create_task`, that
//! names the alternative (docs/sdk.md, "Redirect messages"). The engine
//! ships one default per group; the host replaces or blanks it per group
//! ([`Vm::set_hint`]). Overrides live in the prelude's globals, so they
//! survive `reset` and snapshots.

use crate::types::T;
use crate::vm::{Raised, Vm};
use alloc::{string::String, vec::Vec};
use mm_core::Value;

/// A group of unsupported modules or features that shares one hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HintGroup {
    /// Third-party data libraries: `numpy`, `pandas`, `scipy`, ...
    DataLibraries,
    /// Network clients: `requests`, `httpx`, `socket`, `urllib.request`, ...
    Network,
    /// Processes and threads: `subprocess`, `threading`, ...
    Processes,
    /// Filesystem modules: `shutil`, `glob`, `sqlite3`, `pickle`, ...
    Filesystem,
    /// Third-party time zone libraries: `dateutil`, `pytz`.
    Timezone,
    /// `pydantic`, `attrs`.
    Validation,
    /// `yaml`.
    Yaml,
    /// `bs4`, `lxml`.
    HtmlParsing,
    /// `xml`, `email`.
    XmlEmail,
    /// A bundled module whose feature this build does not enable.
    NotEnabled,
    /// A name `asyncio` lacks.
    Asyncio,
    /// A refused OS hook (`open`, `Path.*`, `os.environ`, `tempfile`).
    OsHooks,
}

/// Every group with its key, the modules it covers (exact dotted names, or
/// top-level names) and its default sentence. `{module}` stands for the
/// module, `{names}` for `asyncio`'s public names.
const GROUPS: [(HintGroup, &str, &[&str], &str); 12] = [
    (
        HintGroup::DataLibraries,
        "data",
        &[
            "numpy",
            "pandas",
            "scipy",
            "sklearn",
            "polars",
            "pyarrow",
            "matplotlib",
            "seaborn",
            "sympy",
            "networkx",
            "torch",
        ],
        "micro-monty runs a Python subset without third-party packages; use lists, dicts and \
         the math, statistics, csv and json modules, or a tool",
    ),
    (
        HintGroup::Network,
        "network",
        &[
            "requests",
            "httpx",
            "aiohttp",
            "urllib3",
            "urllib.request",
            "urllib.error",
            "http",
            "socket",
            "ssl",
            "smtplib",
            "ftplib",
            "websockets",
        ],
        "There is no network in this sandbox; call a tool to fetch data",
    ),
    (
        HintGroup::Processes,
        "processes",
        &[
            "subprocess",
            "multiprocessing",
            "threading",
            "_thread",
            "concurrent",
            "signal",
            "pty",
        ],
        "There are no processes or threads in this sandbox; run commands with a tool, and run \
         tool calls concurrently with asyncio.gather",
    ),
    (
        HintGroup::Filesystem,
        "filesystem",
        &[
            "shutil",
            "glob",
            "zipfile",
            "tarfile",
            "gzip",
            "bz2",
            "lzma",
            "sqlite3",
            "pickle",
            "shelve",
            "fileinput",
        ],
        "There is no filesystem in this sandbox; read and write files with a tool",
    ),
    (
        HintGroup::Timezone,
        "timezone",
        &["dateutil", "pytz", "tzdata"],
        "Use zoneinfo.ZoneInfo, or datetime with timezone.utc or fixed timedelta offsets",
    ),
    (
        HintGroup::Validation,
        "validation",
        &["pydantic", "attrs", "attr", "marshmallow"],
        "Use dataclasses",
    ),
    (
        HintGroup::Yaml,
        "yaml",
        &["yaml", "ruamel"],
        "Use json, or tomllib for TOML",
    ),
    (
        HintGroup::HtmlParsing,
        "html",
        &["bs4", "lxml", "html5lib"],
        "Use re and str methods on the text, or a tool",
    ),
    (
        HintGroup::XmlEmail,
        "xml",
        &["xml", "email"],
        "micro-monty does not bundle '{module}'; use re and str methods on the text, or a tool",
    ),
    (
        HintGroup::NotEnabled,
        "not_enabled",
        &[],
        "'{module}' is not enabled in this build of micro-monty",
    ),
    (
        HintGroup::Asyncio,
        "asyncio",
        &[],
        "micro-monty's asyncio has {names}; pass coroutines to asyncio.gather",
    ),
    (
        HintGroup::OsHooks,
        "os",
        &[],
        "Use a tool for files, environment variables and commands",
    ),
];

impl HintGroup {
    /// Every group.
    pub const ALL: [HintGroup; 12] = [
        HintGroup::DataLibraries,
        HintGroup::Network,
        HintGroup::Processes,
        HintGroup::Filesystem,
        HintGroup::Timezone,
        HintGroup::Validation,
        HintGroup::Yaml,
        HintGroup::HtmlParsing,
        HintGroup::XmlEmail,
        HintGroup::NotEnabled,
        HintGroup::Asyncio,
        HintGroup::OsHooks,
    ];

    fn entry(
        self,
    ) -> &'static (
        HintGroup,
        &'static str,
        &'static [&'static str],
        &'static str,
    ) {
        let i = GROUPS.iter().position(|g| g.0 == self).unwrap_or(0);
        &GROUPS[i]
    }

    /// The group's key (what `_mm.hint` takes).
    #[must_use]
    pub fn key(self) -> &'static str {
        self.entry().1
    }

    /// The modules the group covers (empty for the computed groups).
    #[must_use]
    pub fn modules(self) -> &'static [&'static str] {
        self.entry().2
    }

    /// The default sentence, with its `{module}`/`{names}` placeholders.
    #[must_use]
    pub fn default_text(self) -> &'static str {
        self.entry().3
    }

    fn from_key(key: &str) -> Option<HintGroup> {
        GROUPS.iter().find(|g| g.1 == key).map(|g| g.0)
    }
}

impl Vm {
    /// Sets every bundled module name, enabled in this build or not, so
    /// that importing a disabled one says so ([`HintGroup::NotEnabled`]).
    pub fn set_known_modules(&mut self, names: &'static [&'static str]) {
        self.known_modules = names;
    }

    /// Replaces group `group`'s sentence (`Some("")` blanks it, `None`
    /// restores the default). `{module}` in it stands for the module.
    pub fn set_hint(&mut self, group: HintGroup, text: Option<&str>) -> Result<(), Raised> {
        let d = self.hint_dict()?;
        let k = self.intern(group.key())?;
        match text {
            Some(t) => {
                let v = self.new_str(t)?;
                self.dict_set(d, k, v)
            }
            None => self.dict_del(d, k).map(|_| ()),
        }
    }

    /// The prelude's override table, made on first use.
    fn hint_dict(&mut self) -> Result<Value, Raised> {
        let k = self.intern("_hints")?;
        let pg = self.prelude_globals;
        if let Some(d) = self.dict_get_str(pg, k) {
            return Ok(d);
        }
        let d = self.new_dict()?;
        self.dict_set(pg, k, d)?;
        Ok(d)
    }

    /// Group `group`'s sentence for `module`, or empty when blanked.
    pub(crate) fn hint_text(&mut self, group: HintGroup, module: &str) -> String {
        let over = self
            .find_interned("_hints")
            .and_then(|k| self.dict_get_str(self.prelude_globals, k))
            .zip(self.find_interned(group.key()))
            .and_then(|(d, k)| self.dict_get_str(d, k))
            .filter(|v| Vm::is_str(*v))
            .map(|v| String::from(self.str_ref_o(v)));
        let text = over.unwrap_or_else(|| String::from(group.default_text()));
        let mut text = text.replace("{module}", module);
        if text.contains("{names}") {
            text = text.replace("{names}", &asyncio_names());
        }
        text
    }
}

/// `asyncio`'s public functions and classes in prose, natives and the
/// prelude's (`Semaphore, as_completed, ... and wait_for`); empty in a
/// build without `asyncio`.
#[must_use]
pub fn asyncio_names() -> String {
    #[allow(unused_mut, reason = "the prelude's names only with `asyncio`")]
    let mut names: Vec<&str> = crate::builtins::NATIVES
        .iter()
        .filter(|n| n.module() == Some("asyncio") && !n.name().starts_with('_'))
        .map(crate::builtins::Native::name)
        .collect();
    #[cfg(feature = "asyncio")]
    names.extend(crate::modules::ASYNCIO_PRELUDE);
    crate::ops::heap_sort_by(&mut names, |a, b| a < b);
    names.dedup();
    let mut out = String::new();
    for (i, n) in names.iter().enumerate() {
        if i > 0 {
            out.push_str(if i + 1 == names.len() { " and " } else { ", " });
        }
        out.push_str(n);
    }
    out
}

impl Vm {
    /// Appends `group`'s sentence (if not blanked) to the pending
    /// exception's report.
    pub(crate) fn add_hint(&mut self, group: HintGroup, module: &str) {
        let text = self.hint_text(group, module);
        if !text.is_empty() {
            self.set_hint_suffix(&alloc::format!(". {text}"));
        }
    }

    /// The hint of `No module named 'name'`, if one applies.
    pub(crate) fn import_hint(&mut self, name: &str) {
        let top = name.split('.').next().unwrap_or(name);
        // The engine's own `asyncio` and `json`, off in this build, are
        // reported as a disabled bundled module is.
        let engine_off = matches!(name, "asyncio" | "json") && !crate::BUNDLED.contains(&name);
        if self.known_modules.contains(&name) || engine_off {
            self.add_hint(HintGroup::NotEnabled, name);
            return;
        }
        let group = HintGroup::ALL.iter().copied().find(|g| {
            g.modules()
                .iter()
                .any(|m| *m == name || (!m.contains('.') && *m == top))
        });
        if let Some(g) = group {
            self.add_hint(g, top);
        }
    }

    /// The \"Did you mean\" of a missing module attribute, as CPython 3.14
    /// gives it, else the asyncio hint for a missing `asyncio` name.
    pub(crate) fn module_attr_hint(&mut self, m: Value, name: Value, import: bool) {
        let wrong = String::from(self.str_ref_o(name));
        // asyncio's streams, sockets, subprocesses and threads stay out of
        // scope: point at the host instead of at a near name.
        if self.module_name(m) == "asyncio" {
            let group = match wrong.as_str() {
                "open_connection"
                | "start_server"
                | "open_unix_connection"
                | "start_unix_server"
                | "StreamReader"
                | "StreamWriter" => Some(HintGroup::Network),
                "create_subprocess_exec"
                | "create_subprocess_shell"
                | "subprocess"
                | "to_thread"
                | "run_coroutine_threadsafe" => Some(HintGroup::Processes),
                _ => None,
            };
            if let Some(g) = group {
                let text = self.hint_text(g, "asyncio");
                let note = if text.is_empty() {
                    Value::NONE
                } else {
                    let e = self.exc;
                    let v = self
                        .new_str(&alloc::format!(". {text}"))
                        .unwrap_or(Value::NONE);
                    self.exc = e;
                    v
                };
                self.set_module_hint_pending(m, name, import, note);
                return;
            }
        }
        self.set_module_hint_pending(m, name, import, Value::NONE);
    }

    /// The pending hint of a missing module attribute, when rendered: the
    /// nearest name, else asyncio's sentence for `asyncio`.
    pub(crate) fn module_attr_hint_text(&mut self, m: Value, name: Value) -> Option<String> {
        let wrong = String::from(self.str_ref_o(name));
        let d = self.vslot_o(m, crate::modules::module_slot::DICT);
        let keys: Vec<Value> = self.dict_items_vec(d).into_iter().map(|(k, _)| k).collect();
        let owned = self.hint_names(&keys, &wrong).unwrap_or_default();
        // CPython suggests from `dir(module)`, sorted: the first of equally
        // near names wins.
        let mut cands: Vec<&str> = owned.iter().map(String::as_str).collect();
        crate::ops::heap_sort_by(&mut cands, |a, b| a < b);
        if let Some(s) = self.suggest_charged(&cands, &wrong) {
            return Some(alloc::format!(". Did you mean: '{s}'?"));
        }
        if self.module_name(m) == "asyncio" {
            let text = self.hint_text(HintGroup::Asyncio, "asyncio");
            if !text.is_empty() {
                return Some(alloc::format!(". {text}"));
            }
        }
        None
    }
}

/// `_mm.hint(group)`: group `group`'s sentence, or `''` when blanked.
pub(crate) fn mm_hint(vm: &mut Vm, a: crate::vm::CallArgs) -> Result<Value, Raised> {
    let k = vm.arg_o(&a, 0);
    let group = Vm::is_str(k)
        .then(|| HintGroup::from_key(vm.str_ref_o(k)))
        .flatten();
    let Some(group) = group else {
        return Err(vm.err(T::ValueError, "unknown hint group"));
    };
    let text = vm.hint_text(group, "");
    vm.new_str(&text)
}
