#!/bin/sh
# Restore and snapshot cost per MiB of image, natively, under wasmtime and
# on V8 (README.md). Usage, from anywhere:
#
#     bench/evidence/restore-cost/run.sh [PASSES]     (default 3)
#
# wasmtime comes from $WASMTIME, ~/.local/bin or PATH, and node from PATH;
# a missing one is skipped. Run it on an otherwise idle machine.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
passes=${1:-3}
cd "$here"
cargo build --quiet --release --locked
cargo build --quiet --release --locked --target wasm32-wasip1
native=target/release/restore-cost
wasm=target/wasm32-wasip1/release/restore-cost.wasm
wasmtime=${WASMTIME:-}
[ -n "$wasmtime" ] || { [ -x "$HOME/.local/bin/wasmtime" ] && wasmtime=$HOME/.local/bin/wasmtime; } || true
[ -n "$wasmtime" ] || wasmtime=$(command -v wasmtime || true)
node=$(command -v node || true)
out=$(mktemp -d)
i=1
while [ "$i" -le "$passes" ]; do
    echo "pass $i of $passes" >&2
    "$native" > "$out/native.$i"
    if [ -n "$wasmtime" ]; then
        "$wasmtime" run -W max-memory-size=900000000 "$wasm" > "$out/wasmtime.$i"
    fi
    if [ -n "$node" ]; then
        "$node" --no-warnings --no-liftoff run.mjs "$wasm" > "$out/v8.$i"
    fi
    i=$((i + 1))
done
[ -n "$wasmtime" ] || echo "wasmtime not found: skipped" >&2
[ -n "$node" ] || echo "node not found: skipped" >&2
echo "commit $(git rev-parse --short HEAD), $(nproc) cores, $passes passes; $("${wasmtime:-true}" --version 2>/dev/null || true); node $("${node:-true}" --version 2>/dev/null || true)"
python3 -I summarize.py "$out"
rm -r "$out"
