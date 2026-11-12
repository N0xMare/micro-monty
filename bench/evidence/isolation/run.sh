#!/usr/bin/env bash
# Build every variant of the isolation harness and run the measurements
# (docs/research/isolation-modes.md). Run on an idle machine.
#
#   bash bench/evidence/isolation/run.sh [OUT_DIR] [perf|faults|all]
#
# Builds land in target/isolation (release, release-unwind, wasm32-wasip1)
# and target/isolation-shipped (panic=immediate-abort, std rebuilt as in
# docs/spec.md §6). Nothing here touches the root workspace's build.
set -euo pipefail
here=$(dirname "$(realpath "$0")")
root=$(realpath "$here/../../..")
# Portable (BSD realpath has no -m): create the directory, then resolve it.
out=${1:-$here/results/$(date +%F)}
what=${2:-all}
mkdir -p "$out"
out=$(cd "$out" && pwd)
cd "$here"

T=$root/target/isolation
S=$root/target/isolation-shipped
host=$(rustc -vV | sed -n 's/^host: //p')

cargo build --release --locked -p iso-harness -p iso-worker
cargo build --release --locked -p iso-guest --target wasm32-wasip1
# The mm-wasm v1 guests (crates/mm-wasm-guest, a root-workspace crate, built
# there into the same target directories), with panic injection for the
# fault matrix.
guest() { (cd "$root" && CARGO_TARGET_DIR=${GUEST_TARGET:-$T} "$@"); }
for t in wasm32-wasip1 wasm32-unknown-unknown; do
  guest cargo build --release --locked -p mm-wasm-guest --features test-hooks --target $t
done
cargo build --profile release-unwind --locked -p iso-harness -p iso-worker
shipped() {
  RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort -Cforce-unwind-tables=no --cfg mm_shipped" \
    CARGO_TARGET_DIR=$S cargo build --release --locked -Zbuild-std=std,panic_abort "$@"
}
shipped --target "$host" -p iso-worker
shipped --target "$host" -p iso-harness --no-default-features
shipped --target wasm32-wasip1 -p iso-guest
GUEST_TARGET=$S guest env RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort -Cforce-unwind-tables=no --cfg mm_shipped" \
  cargo build --release --locked -Zbuild-std=std,panic_abort --target wasm32-unknown-unknown -p mm-wasm-guest --features test-hooks
# Size: shipped guests without test-hooks, then wasm-opt -Oz with exactly the
# features rustc enables (as `cargo xtask size`).
Z=$root/target/isolation-size
wasm_opt=${WASM_OPT:-$(command -v wasm-opt || echo "$HOME/.local/bin/wasm-opt")}
features="--enable-bulk-memory --enable-bulk-memory-opt --enable-call-indirect-overlong \
  --enable-multivalue --enable-mutable-globals --enable-nontrapping-float-to-int \
  --enable-reference-types --enable-sign-ext"
mkdir -p "$out"
: > "$out/size.txt"
for t in wasm32-wasip1 wasm32-unknown-unknown; do
  GUEST_TARGET=$Z guest env RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort -Cforce-unwind-tables=no --cfg mm_shipped" \
    cargo build --release --locked -Zbuild-std=std,panic_abort --target $t -p mm-wasm-guest
  m=$Z/$t/release/mm_wasm_guest.wasm
  # shellcheck disable=SC2086
  "$wasm_opt" -Oz $features --strip-debug --strip-producers "$m" -o "${m%.wasm}.opt.wasm"
  printf '%s mm_wasm_guest.wasm %s bytes, %s after wasm-opt -Oz\n' "$t" \
    "$(wc -c < "$m" | tr -d ' ')" "$(wc -c < "${m%.wasm}.opt.wasm" | tr -d ' ')" >> "$out/size.txt"
done

export ISO_ROOT=$root
export ISO_HARNESS=$T/release/iso-harness
export ISO_HARNESS_UNWIND=$T/release-unwind/iso-harness
export ISO_HARNESS_SHIPPED=$S/$host/release/iso-harness
export ISO_WORKER=$T/release/iso-worker
export ISO_WORKER_UNWIND=$T/release-unwind/iso-worker
export ISO_WORKER_SHIPPED=$S/$host/release/iso-worker
export ISO_GUEST=$T/wasm32-wasip1/release/iso_guest.wasm
export ISO_GUEST_SHIPPED=$S/wasm32-wasip1/release/iso_guest.wasm
export ISO_ABI_WASI=$T/wasm32-wasip1/release/mm_wasm_guest.wasm
export ISO_ABI_UU=$T/wasm32-unknown-unknown/release/mm_wasm_guest.wasm
export ISO_ABI_UU_SHIPPED=$S/wasm32-unknown-unknown/release/mm_wasm_guest.wasm
# Precompiled modules are cached next to the .wasm per configuration;
# drop stale ones so they match this build.
rm -f "$T"/wasm32-*/release/*.cwasm "$S"/wasm32-*/release/*.cwasm

{
  git -C "$root" rev-parse HEAD
  rustc -V
  getconf _NPROCESSORS_ONLN
  uname -a
  if command -v sw_vers >/dev/null; then sw_vers; fi
  # macOS before 15 has only shasum.
  sum=sha256sum
  command -v sha256sum >/dev/null || sum="shasum -a 256"
  $sum "$ISO_HARNESS" "$ISO_WORKER" "$ISO_GUEST" "$ISO_HARNESS_UNWIND" "$ISO_HARNESS_SHIPPED" \
    "$ISO_WORKER_UNWIND" "$ISO_WORKER_SHIPPED" "$ISO_GUEST_SHIPPED" \
    "$ISO_ABI_WASI" "$ISO_ABI_UU" "$ISO_ABI_UU_SHIPPED"
} > "$out/build.txt"

# 1024 workers need 2048+ pipe descriptors in the host.
ulimit -n "$(ulimit -Hn)"

if [ "$what" = perf ] || [ "$what" = all ]; then
  # One process per section so RSS readings start clean.
  for section in cold warm hostcall bench snapshot cross memory scale; do
    "$ISO_HARNESS" perf "$section" | tee -a "$out/perf.jsonl"
  done
  # The shipped, wasm-opt'd unknown-unknown guest (no test-hooks).
  ISO_ABI_UU=$Z/wasm32-unknown-unknown/release/mm_wasm_guest.opt.wasm ISO_MODES=abi-uu \
    "$ISO_HARNESS" perf hostcall bench | jq -c '.mode = "abi-uu-opt"' | tee -a "$out/perf.jsonl"
fi
if [ "$what" = faults ] || [ "$what" = all ]; then
  python3 "$here/faults.py" "$out"
fi
python3 "$here/report.py" "$out" > "$out/report.md"
echo "results in $out"
