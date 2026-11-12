#!/usr/bin/env bash
# Instruction counts of the spike under unrelated edits (cargo features
# perturb_*; see Cargo.toml). usage: tools/stability.sh [out.csv]
set -euo pipefail
cd "$(dirname "$0")/.."
out=${1:-results/stability.csv}
KS="fib closure callfn loopfn loop dictint"
FEATS="none perturb_arm perturb_order perturb_field perturb_cold perturb_slow"
for f in $FEATS; do
  if [ $f = none ]; then cargo build -q --release --target-dir target-p-none; else cargo build -q --release --features $f --target-dir target-p-$f; fi
done
for f in $FEATS; do for k in $KS; do echo "$f $k"; done; done | xargs -P 4 -L1 bash -c '
  n=$(valgrind --tool=callgrind --callgrind-out-file=/dev/null target-p-$0/release/dispatch2 run $1 2>&1 >/dev/null | sed -n "s/.*Collected : //p")
  s=$(stat -c %s target-p-$0/release/dispatch2)
  echo "$0,$1,$n"' | sort > "$out"
cat "$out"
