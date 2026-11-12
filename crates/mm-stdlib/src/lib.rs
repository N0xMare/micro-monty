//! Bundled micro-monty stdlib modules.
//!
//! An internal crate of [micro-monty](https://crates.io/crates/micro-monty),
//! published only because the facade depends on it: its API has no semver
//! guarantees and changes with every release. Depend on `micro-monty`.
//!
//! Each module sits behind its own `mod-*` cargo feature. Python modules
//! live in `python/` and are compiled to bytecode by this crate's build
//! script; [`MODULES`] embeds them, and the engine imports each on first
//! `import`. Native parts register through `mm-engine`'s tables.
//!
//! See `docs/spec.md` §12.

#![no_std]

include!(concat!(env!("OUT_DIR"), "/modules.rs"));

/// The enabled `mod-*` features as a bitmask, in `Cargo.toml` order (part
/// of a snapshot's build id).
pub const FEATURES: u64 = cfg!(feature = "mod-json") as u64
    // Bit 1 was `mod-math`; `math` is an engine module, always built.
    | (cfg!(feature = "mod-re") as u64) << 2
    | (cfg!(feature = "mod-collections") as u64) << 3
    | (cfg!(feature = "mod-itertools") as u64) << 4
    | (cfg!(feature = "mod-functools") as u64) << 5
    | (cfg!(feature = "mod-datetime") as u64) << 6
    | (cfg!(feature = "mod-random") as u64) << 7
    | (cfg!(feature = "mod-asyncio") as u64) << 8
    | (cfg!(feature = "mod-typing") as u64) << 9
    | (cfg!(feature = "mod-base64") as u64) << 10
    | (cfg!(feature = "mod-hashlib") as u64) << 11
    | (cfg!(feature = "mod-dataclasses") as u64) << 12
    | (cfg!(feature = "mod-operator") as u64) << 13
    | (cfg!(feature = "mod-copy") as u64) << 14
    | (cfg!(feature = "mod-heapq") as u64) << 15
    | (cfg!(feature = "mod-bisect") as u64) << 16
    | (cfg!(feature = "mod-string") as u64) << 17
    | (cfg!(feature = "mod-textwrap") as u64) << 18
    | (cfg!(feature = "mod-enum") as u64) << 19
    | (cfg!(feature = "mod-abc") as u64) << 20
    | (cfg!(feature = "mod-binascii") as u64) << 21
    | (cfg!(feature = "mod-time") as u64) << 22
    | (cfg!(feature = "mod-os") as u64) << 23
    | (cfg!(feature = "mod-pathlib") as u64) << 24
    | (cfg!(feature = "mod-fnmatch") as u64) << 25
    | (cfg!(feature = "mod-posixpath") as u64) << 26
    | (cfg!(feature = "mod-csv") as u64) << 27
    | (cfg!(feature = "mod-io") as u64) << 28
    | (cfg!(feature = "mod-fractions") as u64) << 29
    | (cfg!(feature = "mod-statistics") as u64) << 30
    | (cfg!(feature = "mod-uuid") as u64) << 31
    | (cfg!(feature = "mod-gc") as u64) << 32
    | (cfg!(feature = "mod-types") as u64) << 33
    | (cfg!(feature = "mod-tomllib") as u64) << 34
    | (cfg!(feature = "mod-decimal") as u64) << 35
    | (cfg!(feature = "mod-numbers") as u64) << 36
    | (cfg!(feature = "mod-contextvars") as u64) << 37
    | (cfg!(feature = "mod-unicodedata") as u64) << 38
    | (cfg!(feature = "mod-contextlib") as u64) << 39
    | (cfg!(feature = "mod-traceback") as u64) << 40
    | (cfg!(feature = "mod-logging") as u64) << 41
    | (cfg!(feature = "mod-urllib") as u64) << 42
    | (cfg!(feature = "mod-pprint") as u64) << 43
    | (cfg!(feature = "mod-shlex") as u64) << 44
    | (cfg!(feature = "mod-difflib") as u64) << 45
    | (cfg!(feature = "mod-struct") as u64) << 46
    | (cfg!(feature = "mod-html") as u64) << 47
    | (cfg!(feature = "mod-html-entities") as u64) << 48
    | (cfg!(feature = "mod-warnings") as u64) << 49
    | (cfg!(feature = "mod-codecs") as u64) << 50
    | (cfg!(feature = "mod-platform") as u64) << 51
    | (cfg!(feature = "mod-tempfile") as u64) << 52
    | (cfg!(feature = "mod-calendar") as u64) << 53
    | (cfg!(feature = "mod-hmac") as u64) << 54
    | (cfg!(feature = "mod-zlib") as u64) << 55
    | (cfg!(feature = "mod-zoneinfo") as u64) << 56
    | (cfg!(feature = "mod-ast") as u64) << 57
    | (cfg!(feature = "mod-tokenize") as u64) << 58;
