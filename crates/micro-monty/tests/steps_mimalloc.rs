//! The step-time oracle (`steps.rs`) on musl with mimalloc as the global
//! allocator, as `mm` and `mm-cell-worker` allocate there
//! (docs/user-guide.md, "musl"); `steps.rs` itself times musl's `malloc`,
//! what an embedder that registers no allocator gets. The two differ row
//! by row: mimalloc hands out memory it has used before, where musl maps
//! every large block fresh, so a 64 MiB result costs 30 ms more with one
//! and a table that grows 30 ms more with the other \[M\]. `cargo xtask
//! check`'s `steps-musl` stage runs both. Rows marked `OpenOnMimalloc` are
//! only reported here.

#![cfg(target_env = "musl")]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[path = "steps.rs"]
mod rows;
