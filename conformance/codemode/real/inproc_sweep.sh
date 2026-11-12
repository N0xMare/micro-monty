#!/bin/sh
# Phase 2 load sweep: plak's CellClient driving N concurrent sessions in
# five modes (Monty child, mm child, mm native in process, mm wasm in
# process, and mm native in process with the decoded-stdlib cache patch).
#
#   inproc_sweep.sh OUT_DIR CORPUS_DIR "1 8 40" REPS
#
# MODES (default all five) picks modes, e.g. MODES="monty mm native wasm".
#
# Needs the two `inproc_load` builds (plak branch mm-inproc and
# mm-inproc-cache, `cargo build --release -p plakous-cell --features
# mm-inproc --example inproc_load`; the source is inproc_load.rs here) and
# the binaries named below. Run it on an otherwise idle machine.
set -eu
out=$1; corpus=$2; sizes=$3; reps=$4
H=${H:-$HOME/dev/plak-mm-inproc/target-load/release/examples/inproc_load}
HC=${HC:-$HOME/dev/plak-mm-inproc-cache/target-load/release/examples/inproc_load}
MONTY=${MONTY:-$(readlink -f "$HOME/plak-mm-root/.local/bin/plakous-py-host")}
MM=${MM:-$HOME/dev/mm-real-raw/bin/mm-cell-worker}
export PLAK_PY_INPROC_WASM=${PLAK_PY_INPROC_WASM:-$HOME/dev/mm-real-raw/bin/mm_wasm_guest.wasm}
mkdir -p "$out"
for rep in $(seq 1 "$reps"); do
  for n in $sizes; do
    for mode in ${MODES:-monty mm native wasm native-cache}; do
      log="$out/$mode-n$n-r$rep.jsonl"
      case $mode in
        monty) env -u PLAK_PY_INPROC PLAK_PY_HOST="$MONTY" "$H" monty "$n" "$corpus" "$log" ;;
        mm) env -u PLAK_PY_INPROC PLAK_PY_HOST="$MM" PLAK_PY_HOST_IDENTITY=micro-monty/micro-monty-cell-adapter.v1 "$H" mm "$n" "$corpus" "$log" ;;
        native|wasm) PLAK_PY_INPROC=$mode PLAK_PY_HOST="$MM" PLAK_PY_HOST_IDENTITY=micro-monty/micro-monty-cell-adapter.v1 "$H" "$mode" "$n" "$corpus" "$log" ;;
        native-cache) PLAK_PY_INPROC=native PLAK_PY_HOST="$MM" PLAK_PY_HOST_IDENTITY=micro-monty/micro-monty-cell-adapter.v1 "$HC" native-cache "$n" "$corpus" "$log" ;;
      esac
    done
  done
done >> "$out/summary.jsonl"
