//! The snapshot reader (docs/spec.md §9.3, §13): `Engine::restore` on raw
//! bytes and on mutated valid snapshots must never panic, and a session
//! that restores must run a bounded amount of fuel, save itself again and
//! run a new cell without panicking.
//!
//! The first input byte picks the mode: raw bytes; or edits to one of a
//! few valid snapshots (taken idle, parked deep in generators and methods,
//! at a host call, waiting on host futures, parked inside tasks), anywhere
//! or in the VM-state tail, with the checksum recomputed so the edits reach
//! the decoder and the engine (one mode in four leaves it stale). Typed
//! host modules give the images code the engine shares by key
//! (`CODE_HOST`); other mode bits edit the image that stores that code
//! instead, or restore on an engine without a module cache, which builds
//! the code again from the image's definitions and checks its key; another
//! restores on both engines and requires the same verdict on the host code
//! (the cached engine takes its code by key only for the definition it was
//! built from). Picks 0xE0..=0xEF forge the header's stop to another phase
//! than the VM is stopped at first, which a restore must refuse.
//!
//! ```bash
//! cd fuzz && cargo +nightly fuzz run snapshot_restore
//! ```

#![no_main]

#[path = "snapshot_restore_input.rs"]
mod input;

libfuzzer_sys::fuzz_target!(|data: &[u8]| input::run(data));
