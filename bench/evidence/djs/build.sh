#!/usr/bin/env bash
# Build an external public-API probe without editing the comparator checkout.
set -euo pipefail
here=$(dirname "$(realpath "$0")")
source_dir=${DJS_SOURCE:-/tmp/mm-evidence-djs/source}
state_dir=/tmp/mm-evidence-djs
revision=548ab4874a08d85f3a6f0b07feaa510df1aaf969
test "$(git -C "$source_dir" rev-parse HEAD)" = "$revision"
test -z "$(git -C "$source_dir" status --porcelain)"
mkdir -p "$state_dir/probe"
cat > "$state_dir/probe/Cargo.toml" <<EOF
[package]
name = "mm-evidence-djs-probe"
version = "0.0.0"
edition = "2024"

[workspace]

[[bin]]
name = "djs-probe"
path = "$here/main.rs"

[dependencies]
djs-core = { path = "$source_dir/crates/djs-core" }
djs-prelude = { path = "$source_dir/crates/djs-prelude" }

[profile.release]
opt-level = 3
debug = 1
EOF
cargo +1.99.0 generate-lockfile --offline --manifest-path "$state_dir/probe/Cargo.toml"
CARGO_TARGET_DIR="$state_dir/probe-target" cargo +1.99.0 build --release --locked \
  --manifest-path "$state_dir/probe/Cargo.toml" -j 2
