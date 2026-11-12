#!/usr/bin/env bash
# Reproduce every number in docs/spikes/dispatch.md. Benchmarks run one at a
# time, pinned to one CPU. Outputs go to results/.
#
#   ./run_all.sh                  # from spikes/dispatch/
#   REPS=15 PIN="taskset -c 2" PY=~/.local/bin/python3.12 QJS=/tmp/dquickts/qjs ./run_all.sh
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
PY=${PY:-$HOME/.local/bin/python3.12}
QJS=${QJS:-/tmp/dquickts/qjs}
REPS=${REPS:-15}
PIN=${PIN:-taskset -c 2}
mkdir -p results

echo "== build"
cargo build --release
cargo build --release --features unchecked --target-dir target-unchecked
cargo build --release --features count --target-dir target-count
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --target-dir target-syms

echo "== environment"
{
  echo "date: $(date -u +%FT%TZ)"
  echo "uptime: $(uptime)"
  lscpu | grep -E 'Model name|^CPU\(s\)|Thread|Core|Socket|L1d|L1i|L2|L3|Hypervisor'
  rustc -vV
  echo "python: $($PY --version)"
  echo "qjs: $QJS (dquickts $(git -C "$(dirname "$QJS")" rev-parse HEAD 2>/dev/null || echo '?'), make qjs, -O2)"
  gcc --version | head -1
  echo "pin: $PIN"
} > results/env.txt

echo "== dispatch bench (checked, default features)"
$PIN ./target/release/dispatch bench --reps "$REPS" > results/bench_checked.txt
echo "== dispatch bench (--features unchecked)"
$PIN ./target-unchecked/release/dispatch bench --reps "$REPS" > results/bench_unchecked.txt
echo "== dispatch bench (checked, second process, for run-to-run spread)"
$PIN ./target/release/dispatch bench --reps "$REPS" > results/bench_checked_run2.txt
echo "== stats (--features count)"
./target-count/release/dispatch stats > results/stats.txt
echo "== handler sizes"
python3 tools/handler_sizes.py target-syms/release/dispatch > results/handler_sizes.csv
echo "== CPython"
$PIN "$PY" kernels.py "$REPS" > results/cpython.txt
echo "== qjs"
$PIN "$QJS" kernels.js "$REPS" > results/qjs.txt
echo "== perf"
{
  if command -v perf > /dev/null; then
    perf stat -e instructions,cycles,branch-misses ./target/release/dispatch bench --reps 1 --kernel fib --variant stack/match 2>&1 | grep -v '^verify' || true
  else
    echo "perf not installed"
  fi
} > results/perf.txt 2>&1

echo "== summary"
python3 tools/summarize.py results > results/summary.md
cat results/summary.md
