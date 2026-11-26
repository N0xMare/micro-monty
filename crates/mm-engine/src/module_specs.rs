//! The import-time recipes for native namespaces, packed next to the
//! native table. Nothing in the manifest below runs in the guest.

use super::{COUNT, DEFS, PACKED, Packed, Sref, T, add_str, prefix, str_eq};
use core::ops::Range;

/// Top-level native modules (including `_mm`, private to the engine).
pub const BUNDLED: &[&str] = &[
    "_mm",
    #[cfg(feature = "asyncio")]
    "asyncio",
    "cmath",
    "importlib",
    #[cfg(feature = "json")]
    "json",
    "math",
    "sys",
];

pub(super) const fn native_group(module: Option<&str>) -> usize {
    if let Some(name) = module {
        let mut i = 0;
        while i < BUNDLED.len() {
            if str_eq(BUNDLED[i], name) {
                return i + 1;
            }
            i += 1;
        }
    }
    0
}

struct Child {
    name: &'static str,
    exports: &'static [&'static str],
}

const CHILDREN: &[Child] = &[
    #[cfg(feature = "asyncio")]
    Child {
        name: "asyncio.events",
        exports: &[
            "Handle",
            "TimerHandle",
            "get_event_loop",
            "get_running_loop",
            "new_event_loop",
            "set_event_loop",
            "_set_event_loop_policy",
        ],
    },
    #[cfg(feature = "asyncio")]
    Child {
        name: "asyncio.exceptions",
        exports: &[
            "BrokenBarrierError",
            "CancelledError",
            "InvalidStateError",
            "TimeoutError",
        ],
    },
    #[cfg(feature = "asyncio")]
    Child {
        name: "asyncio.futures",
        exports: &["Future", "isfuture"],
    },
    #[cfg(feature = "asyncio")]
    Child {
        name: "asyncio.locks",
        exports: &[
            "Lock",
            "Event",
            "Condition",
            "Semaphore",
            "BoundedSemaphore",
            "Barrier",
        ],
    },
    #[cfg(feature = "asyncio")]
    Child {
        name: "asyncio.queues",
        exports: &[
            "Queue",
            "PriorityQueue",
            "LifoQueue",
            "QueueFull",
            "QueueEmpty",
            "QueueShutDown",
        ],
    },
    #[cfg(feature = "asyncio")]
    Child {
        name: "asyncio.runners",
        exports: &["Runner", "run"],
    },
    #[cfg(feature = "asyncio")]
    Child {
        name: "asyncio.taskgroups",
        exports: &["TaskGroup"],
    },
    #[cfg(feature = "asyncio")]
    Child {
        name: "asyncio.tasks",
        exports: &[
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
    },
    #[cfg(feature = "asyncio")]
    Child {
        name: "asyncio.timeouts",
        exports: &["Timeout", "timeout", "timeout_at"],
    },
    #[cfg(feature = "json")]
    Child {
        name: "json.decoder",
        exports: &["JSONDecoder", "JSONDecodeError"],
    },
    #[cfg(feature = "json")]
    Child {
        name: "json.encoder",
        exports: &["JSONEncoder"],
    },
];

#[cfg(feature = "json")]
const JSON_UNITS: &[&str] = &[
    "dumps",
    "loads",
    "dump",
    "load",
    "JSONDecodeError",
    "JSONDecoder",
    "JSONEncoder",
];
const SYS_EXPORTS: &[&str] = &[
    "version_info",
    "float_info",
    "hash_info",
    "flags",
    "int_info",
    "get_int_max_str_digits",
    "set_int_max_str_digits",
    "exit",
];
const SYS_UNITS: &[&str] = &[
    "_version_info",
    "_float_info",
    "_hash_info",
    "_flags",
    "_int_info",
    "get_int_max_str_digits",
    "set_int_max_str_digits",
    "exit",
];
const SYS_TEXT: &[(&str, &str)] = &[
    ("platform", "linux"),
    ("version", "3.14.7 (micro-monty)"),
    ("byteorder", "little"),
    ("float_repr_style", "short"),
    (
        "copyright",
        "Copyright (c) 2001 Python Software Foundation.",
    ),
];
const SYS_INT: &[(&str, i64)] = &[
    ("maxsize", i64::MAX),
    ("maxunicode", 0x0010_FFFF),
    ("hexversion", 0x030E_07F0),
    ("api_version", 1013),
];

#[derive(Clone, Copy)]
#[allow(dead_code, reason = "some recipes need module features")]
pub(crate) enum Binding {
    Lazy,
    Eager(u8),
    Type(T),
    Float(u8),
    Imaginary(u8),
    Text(u8),
    Int(u8),
    None,
    BuiltinNames,
    Argv,
    Path,
    Stream(u8),
    Child(u8),
    Copy(u8),
    All(u8),
}

/// The offset and length in the native string blob. With a zero length
/// the offset is instead the index of a prelude unit's existing name.
#[derive(Clone, Copy)]
struct Key([u8; 3]);

impl Key {
    const EMPTY: Self = Self([0; 3]);

    const fn new(off: u16, len: u8) -> Self {
        let [lo, hi] = off.to_le_bytes();
        Self([lo, hi, len])
    }

    const fn blob(r: Sref) -> Self {
        Self::new(r.off, r.len)
    }

    const fn pack(native: &mut Packed, name: &str) -> Self {
        if let Some(i) = crate::prelude::unit_index(name) {
            Self::new(i, 0)
        } else {
            Self::blob(add_str(native, name))
        }
    }

    const fn eq(self, other: Self) -> bool {
        let [lo, hi, len] = self.0;
        let [a, b, n] = other.0;
        lo == a && hi == b && len == n
    }

    fn name(self) -> &'static str {
        let [lo, hi, len] = self.0;
        let off = u16::from_le_bytes([lo, hi]);
        if len == 0 {
            crate::prelude::name_of(usize::from(off))
        } else {
            Sref { off, len }.get()
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Entry {
    key: Key,
    pub(crate) binding: Binding,
}

impl Entry {
    pub(crate) fn name(self) -> &'static str {
        self.key.name()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Spec {
    name: Sref,
    native_start: u16,
    native_len: u8,
    start: u16,
    len: u8,
}

// Bounds of compile-time scratch, not shipped table lengths.
pub(super) const EXTRA_STR_BYTES: usize = 1024;
const ROOM: usize = 192;
const MODULES: usize = BUNDLED.len() + CHILDREN.len();

pub(super) struct PackedModules {
    specs: [Spec; MODULES],
    entries: [Entry; ROOM],
    entry_len: usize,
    public_prefix: [u8; MODULES],
    max_entries: usize,
}

impl PackedModules {
    pub(super) const EMPTY: Self = Self {
        specs: [Spec {
            name: Sref { off: 0, len: 0 },
            native_start: 0,
            native_len: 0,
            start: 0,
            len: 0,
        }; MODULES],
        entries: [Entry {
            key: Key::EMPTY,
            binding: Binding::None,
        }; ROOM],
        entry_len: 0,
        public_prefix: [0; MODULES],
        max_entries: 0,
    };

    const fn public(&mut self, m: usize, name: &str, at: u8) {
        if !matches!(name.as_bytes(), [b'_', ..]) {
            let len = &mut self.public_prefix[m];
            *len = if *len == at { at + 1 } else { u8::MAX };
        }
    }

    const fn entry(&mut self, native: &mut Packed, m: usize, name: &str, binding: Binding) {
        let key = Key::pack(native, name);
        assert!(
            !matches!(binding, Binding::Lazy) || key.0[2] == 0,
            "no lazy prelude export"
        );
        assert!(
            self.entry_number(m, name, key).is_none(),
            "duplicate bundled module entry"
        );
        let spec = self.specs[m];
        self.public(m, name, spec.len + spec.native_len);
        self.entries[self.entry_len] = Entry { key, binding };
        self.entry_len += 1;
        self.specs[m].len += 1;
    }

    const fn entry_number(&self, m: usize, name: &str, key: Key) -> Option<u8> {
        let spec = self.specs[m];
        let mut i = 0;
        while i < spec.native_len {
            let index = spec.native_start as usize + i as usize;
            if str_eq(DEFS[super::FUNCTION_ORDER.0[index] as usize].name, name) {
                return Some(i);
            }
            i += 1;
        }
        i = 0;
        while i < spec.len {
            if self.entries[spec.start as usize + i as usize].key.eq(key) {
                return Some(spec.native_len + i);
            }
            i += 1;
        }
        None
    }

    const fn all(&mut self, native: &mut Packed, m: usize) {
        // Package recipes list their public exports before private names
        // and child modules, so a count describes the whole tuple.
        let len = self.public_prefix[m];
        assert!(
            len != u8::MAX,
            "public exports must come before private names"
        );
        self.entry(native, m, "__all__", Binding::All(len));
    }
}

// Only these trimmed tables are linked; never take the address of PACKED.
pub(crate) static SPECS: [Spec; MODULES] = PACKED.bundled.specs;
static ENTRIES: [Entry; PACKED.bundled.entry_len] = prefix(&PACKED.bundled.entries);
pub(crate) const MAX_ENTRIES: usize = PACKED.bundled.max_entries;
pub(crate) const CHILD_COUNT: usize = CHILDREN.len();

impl Spec {
    pub(crate) fn name(self) -> &'static str {
        self.name.get()
    }

    pub(crate) fn natives(self) -> Range<usize> {
        let start = usize::from(self.native_start);
        start..start + usize::from(self.native_len)
    }

    pub(crate) fn entries(self) -> &'static [Entry] {
        let start = usize::from(self.start);
        &ENTRIES[start..start + usize::from(self.len)]
    }
}

pub(crate) fn find(s: &str) -> Option<Spec> {
    SPECS.iter().copied().find(|m| m.name() == s)
}

pub(crate) fn eager_units(m: Spec) -> &'static [&'static str] {
    match m.name() {
        "sys" => SYS_UNITS,
        #[cfg(feature = "json")]
        "json" => JSON_UNITS,
        _ => &[],
    }
}

pub(crate) fn text(i: u8) -> &'static str {
    SYS_TEXT[usize::from(i)].1
}

pub(crate) fn integer(i: u8) -> i64 {
    SYS_INT[usize::from(i)].1
}

const fn leaf<'a>(name: &'a str, parent: &str) -> Option<&'a str> {
    let Some((prefix, rest)) = name.as_bytes().split_at_checked(parent.len()) else {
        return None;
    };
    if let [b'.', tail @ ..] = rest
        && let Ok(prefix) = core::str::from_utf8(prefix)
        && str_eq(prefix, parent)
        && let Ok(tail) = core::str::from_utf8(tail)
    {
        return Some(tail);
    }
    None
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "compile-time module and entry bounds"
)]
pub(super) const fn pack(
    native: &mut Packed,
    order: &[u16; COUNT],
    functions: usize,
) -> PackedModules {
    let mut out = PackedModules::EMPTY;
    let mut parents = [usize::MAX; CHILDREN.len()];
    let mut m = 0;
    while m < BUNDLED.len() {
        let name = BUNDLED[m];
        out.specs[m].name = add_str(native, name);
        out.specs[m].start = out.entry_len as u16;
        let mut n = 0;
        while n < COUNT {
            if native_group(DEFS[order[n] as usize].module) == m + 1 {
                assert!(n < functions, "a bundled module native must be a function");
                let s = &mut out.specs[m];
                if s.native_len == 0 {
                    s.native_start = n as u16;
                }
                assert!(s.native_start as usize + s.native_len as usize == n);
                let f = DEFS[order[n] as usize].name;
                let mut i = s.native_start as usize;
                while i < n {
                    assert!(!str_eq(DEFS[order[i] as usize].name, f));
                    i += 1;
                }
                let at = s.native_len;
                s.native_len += 1;
                out.public(m, f, at);
            }
            n += 1;
        }
        extras(&mut out, native, m, name);
        if str_eq(name, "asyncio") {
            out.all(native, m);
        }
        let mut c = 0;
        while c < CHILDREN.len() {
            if let Some(leaf) = leaf(CHILDREN[c].name, name) {
                let i = BUNDLED.len() + c;
                parents[c] = m;
                out.entry(native, m, leaf, Binding::Child(i as u8));
            }
            c += 1;
        }
        m += 1;
    }
    let mut c = 0;
    while c < CHILDREN.len() {
        let m = BUNDLED.len() + c;
        let child = &CHILDREN[c];
        out.specs[m].name = add_str(native, child.name);
        out.specs[m].start = out.entry_len as u16;
        let parent = parents[c];
        let asyncio = str_eq(BUNDLED[parent], "asyncio");
        let mut e = 0;
        while e < child.exports.len() {
            let name = child.exports[e];
            let key = Key::pack(native, name);
            let kind = if let Some(index) = out.entry_number(parent, name, key) {
                Binding::Copy(index)
            } else {
                assert!(asyncio, "missing parent export");
                Binding::Lazy
            };
            out.entry(native, m, name, kind);
            e += 1;
        }
        if asyncio {
            out.all(native, m);
        }
        c += 1;
    }
    let mut m = 0;
    while m < MODULES {
        let s = out.specs[m];
        let len = s.native_len as usize + s.len as usize;
        if len > out.max_entries {
            out.max_entries = len;
        }
        m += 1;
    }
    out
}

#[allow(clippy::cast_possible_truncation, reason = "compile-time table bounds")]
const fn extras(out: &mut PackedModules, native: &mut Packed, m: usize, name: &str) {
    if str_eq(name, "importlib") {
        out.entry(native, m, "import_module", Binding::Lazy);
    }
    #[cfg(feature = "asyncio")]
    if str_eq(name, "asyncio") {
        let mut i = 0;
        while i < crate::modules::ASYNCIO_PRELUDE.len() {
            let name = crate::modules::ASYNCIO_PRELUDE[i];
            out.entry(native, m, name, Binding::Lazy);
            i += 1;
        }
        let types = [
            T::CancelledError,
            T::InvalidStateError,
            T::TimeoutError,
            T::Task,
        ];
        i = 0;
        while i < types.len() {
            out.entry(native, m, types[i].name(), Binding::Type(types[i]));
            i += 1;
        }
    }
    #[cfg(feature = "json")]
    if str_eq(name, "json") {
        let mut i = 0;
        while i < JSON_UNITS.len() {
            assert!(
                crate::prelude::unit_index(JSON_UNITS[i]).is_some(),
                "no JSON prelude export"
            );
            out.entry(native, m, JSON_UNITS[i], Binding::Eager(i as u8));
            i += 1;
        }
    }
    if str_eq(name, "cmath") || str_eq(name, "math") {
        let mut i = 0;
        while i < crate::math::CONSTANTS.len() {
            out.entry(
                native,
                m,
                crate::math::CONSTANTS[i].0,
                Binding::Float(i as u8),
            );
            i += 1;
        }
        if str_eq(name, "cmath") {
            out.entry(native, m, "infj", Binding::Imaginary(3));
            out.entry(native, m, "nanj", Binding::Imaginary(4));
        }
    }
    if str_eq(name, "sys") {
        let mut i = 0;
        while i < SYS_TEXT.len() - 1 {
            out.entry(native, m, SYS_TEXT[i].0, Binding::Text(i as u8));
            i += 1;
        }
        i = 0;
        while i < SYS_INT.len() {
            out.entry(native, m, SYS_INT[i].0, Binding::Int(i as u8));
            i += 1;
        }
        i = 0;
        while i < SYS_EXPORTS.len() {
            assert!(
                crate::prelude::unit_index(SYS_UNITS[i]).is_some(),
                "no sys prelude export"
            );
            out.entry(native, m, SYS_EXPORTS[i], Binding::Eager(i as u8));
            i += 1;
        }
        out.entry(
            native,
            m,
            "copyright",
            Binding::Text((SYS_TEXT.len() - 1) as u8),
        );
        out.entry(native, m, "pycache_prefix", Binding::None);
        out.entry(native, m, "builtin_module_names", Binding::BuiltinNames);
        out.entry(native, m, "argv", Binding::Argv);
        out.entry(native, m, "path", Binding::Path);
        out.entry(native, m, "stdout", Binding::Stream(1));
        out.entry(native, m, "stderr", Binding::Stream(2));
        out.entry(native, m, "modules", Binding::Lazy);
    }
}
