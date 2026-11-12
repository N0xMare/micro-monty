#!/usr/bin/env bash
# Instruction counts (callgrind, whole process) for every engine × program.
# usage: tools/callgrind.sh [out.csv]   env: MM, QJS, D2 (spike binary), PROGS
set -euo pipefail
cd "$(dirname "$0")/.."
R=$(git rev-parse --show-toplevel)
MM=${MM:-$R/target/release/mm}
QJS=${QJS:-/tmp/dquickts/qjs}
D2=${D2:-target/release/dispatch2}
PROGS=${PROGS:-"fib closure callfn loopfn loop dictint"}
out=${1:-results/callgrind.csv}
src() { case $1 in fib|closure|loop) echo "$R/bench/$1.$2";; *) echo "programs/$1.$2";; esac; }
jobs=()
for p in $PROGS empty; do
  jobs+=("mm $p $MM run $(src $p py)")
  jobs+=("qjs $p $QJS $(src $p js)")
  jobs+=("dispatch2 $p $D2 run $p")
done
printf '%s\n' "${jobs[@]}" | xargs -P 4 -I{} bash -c '
  set -- {}; eng=$1; prog=$2; shift 2
  n=$(valgrind --tool=callgrind --trace-children=yes --callgrind-out-file=/tmp/cgx.%p "$@" 2>&1 >/dev/null | sed -n "s/.*Collected : //p" | paste -sd+ | bc)
  echo "$eng,$prog,$n"' | sort > "$out"
cat "$out"
