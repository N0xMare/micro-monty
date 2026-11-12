#!/usr/bin/env bash
# Spike S2: everything the results page reports, sequentially (about 10 minutes).
# env: QJS (dquickts `make qjs`), MM (micro-monty `mm`, built here if absent), CPU (pin, default 2)
set -euo pipefail
cd "$(dirname "$0")"
export PATH=$HOME/.cargo/bin:$PATH
R=$(git rev-parse --show-toplevel)
export MM=${MM:-$R/target/release/mm}
[ -x "$MM" ] || cargo build --release --locked -p mm-cli --manifest-path "$R/Cargo.toml"

cargo build --release
cargo build --release --features count --target-dir target-count
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --target-dir target-syms
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --no-default-features --target-dir target-syms-lean

./target/release/dispatch2 test > results/test.txt
./target-count/release/dispatch2 stats > results/stats.txt
tools/callgrind.sh results/callgrind.csv
tools/stability.sh results/stability.csv
for t in target-syms target-syms-lean; do
  nm -S -C $t/release/dispatch2 | grep -E " [tT] .*(dispatch2::vm::|Vm>::|dispatch2::heap::Heap>::(dict_|collect)|Heap>::(alloc|exc))" |
    awk -v t=$t '{printf "%s,%d,%s\n", t, strtonum("0x"$2), substr($0, index($0,$4))}'
done | sort -t, -k1,1 -k2,2nr > results/code_size.csv
python3 tools/wall.py 21 results/wall_run1.txt
python3 tools/wall.py 21 results/wall_run2.txt
python3 tools/summarize.py results > results/summary.md
cat results/summary.md
