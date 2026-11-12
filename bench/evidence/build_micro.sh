#!/usr/bin/env bash
# Build the unchanged engine's CLI feature set and external public-API probes.
set -euo pipefail
here=$(dirname "$(realpath "$0")")
root=$(realpath "${1:?pass the source checkout}")
out=$(realpath -m "${2:?pass an output directory}")
mkdir -p "$out"
cargo +1.99.0 build --manifest-path "$root/Cargo.toml" --release --locked \
  -p mm-cli --message-format=json > "$out/build.jsonl"
lib=$(jq -r 'select(.reason == "compiler-artifact" and .target.name == "micro_monty") | .filenames[] | select(endswith(".rlib"))' "$out/build.jsonl")
test -f "$lib"
rustc +1.99.0 --edition=2024 -C opt-level=3 -C lto=fat -C codegen-units=1 \
  -C panic=abort -C strip=symbols "$here/micro_monty.rs" \
  -L "dependency=$(dirname "$lib")" --extern "micro_monty=$lib" -o "$out/probe"
rustc +1.99.0 --edition=2024 -C opt-level=3 -C panic=abort "$root/bench/latency.rs" \
  -L "dependency=$(dirname "$lib")" --extern "micro_monty=$lib" -o "$out/latency"
sha256sum "$out/probe" "$out/latency" "$root/target/release/mm" "$lib" \
  > "$out/artifacts.sha256"
printf 'probes built in %s\n' "$out"
