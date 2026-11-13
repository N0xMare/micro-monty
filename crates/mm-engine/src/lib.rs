//! The micro-monty VM and builtins.
//!
//! An internal crate of [micro-monty](https://crates.io/crates/micro-monty),
//! published only because the facade depends on it: its API has no semver
//! guarantees and changes with every release. Depend on `micro-monty`.
//!
//! Owns execution: the dispatch loop, frames on the arena frame stack, the call
//! protocol, exceptions and tracebacks, generators and coroutines, the async
//! scheduler, inline caches, fuel and deadlines, host-call suspension and
//! pause-anywhere parking. Builtin functions and types are static tables with
//! one calling convention.
//!
//! Engine invariant: the Rust stack never holds two dispatch loops. Builtins
//! that can reach user code take a native fast path for builtin-typed data and
//! hand off to prelude bytecode otherwise, so every safepoint can pause.
//!
//! See `docs/spec.md` §8.

#![no_std]
// Shipped builds only (docs/spec.md §6): `cargo xtask size`/`wasm` pass
// `--cfg mm_shipped` with `RUSTC_BOOTSTRAP=1`; stable in Rust 1.101.
#![cfg_attr(mm_shipped, feature(optimize_attribute))]
#![allow(clippy::doc_markdown, reason = "the docs name CPython in prose")]
#![allow(
    clippy::missing_errors_doc,
    reason = "every fallible function raises a Python exception held by the VM"
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "Python numeric semantics need deliberate conversions between i64, u64 and f64"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "numeric code follows CPython's names (v, w, x, y, q, r)"
)]
#![allow(
    clippy::too_many_lines,
    reason = "the dispatch loop and CPython ports read best unsplit"
)]
#![allow(
    clippy::float_cmp,
    reason = "Python float equality is exact IEEE comparison"
)]

/// A hash of what an image's meaning rests on in this crate's sources (the
/// VM-section codec, and every `const` and `#[repr]` item: object, frame,
/// continuation and native-step layouts; comments and blank space left
/// out), part of the snapshot build id (`micro_monty::build_id`,
/// docs/spec.md §9.3; see the build script).
pub(crate) const LAYOUT_HASH: u64 = include!(concat!(env!("OUT_DIR"), "/layout_hash.rs"));

extern crate alloc;

mod aio;
#[cfg(feature = "ast")]
mod astmod;
mod bigint;
#[cfg(feature = "binascii")]
mod binascii;
mod builtins;
mod bytearray;
mod bytes;
mod class;
mod cmath;
mod complex;
mod cont;
#[cfg(feature = "datetime")]
mod datetimemod;
mod dict;
mod dunder;
mod exc;
mod generator;
#[cfg(feature = "hashlib")]
mod hashlib;
mod hints;
mod host;
mod interp;
mod intops;
#[cfg(feature = "json")]
mod json;
mod libm;
mod lifecycle;
mod matching;
mod math;
mod mmmod;
mod modules;
mod nstep;
mod ops;
mod percent;
mod prelude;
#[cfg(feature = "random")]
mod random;
#[cfg(feature = "re")]
mod re;
mod scratch;
mod set;
mod snapshot;
mod strmeth;
#[cfg(feature = "struct")]
mod structmod;
pub mod text;
#[cfg(feature = "tokenize")]
mod tokmod;
pub mod types;
#[cfg(feature = "unicodedata")]
mod unicodedata;
mod vm;
mod walk;

pub use aio::FutureStatus;
pub use hints::{HintGroup, asyncio_names};
pub use modules::{BUNDLED, is_bundled};

pub use builtins::{Native, find_method, method_names, native};
pub use host::{
    CallStatus, HostArgs, HostCall, HostError, HostErrorKind, HostProgram, HostValue,
    MAX_HOST_DEPTH, Unconverted,
};
pub use prelude::{PRELUDE_CHUNKS, prelude_programs};
pub use snapshot::{
    HostCode, NotAtSafepoint, SnapError, build_fingerprint, put_host_value, read_host_value,
};
pub use vm::{
    CallArgs, Clock, DecodedModule, Entropy, Exit, ModuleCache, Output, Packing, ParkReason,
    PreludeChunk, Raised, StdModule, Stopped, Stream, Vm, VmConfig, WallClock, ZoneSource,
};
