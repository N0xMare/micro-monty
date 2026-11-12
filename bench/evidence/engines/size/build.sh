#!/usr/bin/env bash
# Builds the size hosts as `cargo xtask size` builds micro-monty's `hello`
# (xtask/src/main.rs `shipped_cargo`): release profile, std rebuilt with
# immediate abort and no unwind tables, on the repo's pinned toolchain.
set -euo pipefail
cd "$(dirname "$0")"
target=x86_64-unknown-linux-gnu
for host in rquickjs djs; do
  (cd "$host" && RUSTC_BOOTSTRAP=1 \
    RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort -Cforce-unwind-tables=no --cfg mm_shipped" \
    CARGO_TARGET_DIR="${SIZE_TARGET:-/tmp/eng/target-size}-$host" \
    cargo build --release -Zbuild-std=std,panic_abort --target "$target")
done
