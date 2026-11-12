//! The micro-monty heap.
//!
//! An internal crate of [micro-monty](https://crates.io/crates/micro-monty),
//! published only because the facade depends on it: its API has no semver
//! guarantees and changes with every release. Depend on `micro-monty`.
//!
//! Owns everything that lives in a session's memory and nothing that runs:
//!
//! - [`Value`]: 8-byte NaN-boxed values whose references are `u32` offsets
//!   into the arena, never pointers.
//! - The arena ([`Heap`]): fixed-size segments of words for objects and frames,
//!   and of bytes for `str`/`bytes` data. Segments never move, so references
//!   stay valid as the heap grows and a FULL snapshot is the segment list.
//! - Non-moving mark-sweep GC with out-of-line mark bitmaps, run only when the
//!   engine asks ([`Heap::collect`]).
//! - The write barrier every value-slot write goes through: per-segment
//!   flags the incremental mark rescans.
//! - `str` objects, the string hash and interning.
//!
//! Every access is bounds-checked and every error is an [`Error`]; nothing here
//! panics on any input. See `docs/spec.md` §7 and §9.

#![no_std]
// Shipped builds only (docs/spec.md §6); stable in Rust 1.101.
#![cfg_attr(mm_shipped, feature(optimize_attribute))]
#![allow(
    clippy::missing_errors_doc,
    reason = "every fallible method returns the one `Error` enum, documented there"
)]

/// A hash of what an image's meaning rests on in this crate's sources (the
/// heap codec, and every `const` and `#[repr]` item: type ids, header
/// flags, the value encoding; comments and blank space left out), part of
/// the snapshot build id (`micro_monty::build_id`, docs/spec.md §9.3; see
/// the build script).
pub const LAYOUT_HASH: u64 = include!(concat!(env!("OUT_DIR"), "/layout_hash.rs"));

extern crate alloc;

mod gc;
mod heap;
mod snapshot;
mod space;
mod string;
mod value;

pub use gc::Tracer;
pub use heap::{
    ByteRef, Error, GcStats, Header, Heap, HeapConfig, LARGE_OBJECT_BYTES, MAX_OBJECT_BYTES,
    ObjectCursor, Ref, SEGMENT_BYTES, Scratch, encoding_samples, flags, types,
};
pub use snapshot::{Reader, put_bytes, put_len, put_u8, put_u32, put_u64, xxh64};
pub use string::{StrBuf, hash_bytes, hash_bytes_const};
pub use value::{Special, Value};
