cd <checkout>
R=.amp/in/r4; log() { echo "$1 $(date -u +%FT%TZ)" >> $R/progress.txt; }
quiet() { q=0; while [ $q -lt 3 ]; do c=$(ps -Ao pcpu,comm | awk '$2 ~ /(^|\/)plak$/ {s+=$1} END {print int(s)}'); if pgrep -x rustc >/dev/null || pgrep -x cargo >/dev/null || [ "$c" -gt 60 ]; then q=0; else q=$((q+1)); fi; sleep 10; done; echo "quiet plak=$c $(date -u +%FT%TZ)" >> $R/progress.txt; }
D=bench/results/2026-11-06-mac-m1max-aarch64-macos-5349520
quiet; log bench-start; uptime >> $R/progress.txt
QJS=$HOME/src/dquickts/qjs MONTY=$HOME/src/monty-cli/bin/monty cargo xtask bench --out-dir $D > $R/bench.log 2>&1; echo rc=$? >> $R/bench.log; log bench
quiet; log ab-start
A=.amp/in/ab; python3.14 $A/ab.py 40 $A/mac-5349520.json e6d7251=$A/e6d7251/target/release/mm 1fe8e2d=$A/1fe8e2d/target/release/mm c97ce06=$A/c97ce06/target/release/mm 5349520=$A/5349520/target/release/mm -- bench/closure.py bench/fib.py bench/loop.py bench/dict.py > $A/mac-5349520.log 2>&1; log ab
H=target/isolation/release/iso-harness; export ISO_ROOT=$PWD ISO_SOAK_SESSIONS=40
{ git rev-parse HEAD; shasum -a 256 $H target/isolation/wasm32-*/release/mm_wasm_guest.wasm; } > $R/soak-build.txt
for m in abi abi-uu; do ISO_MODES=$m ISO_SOAK_SECS=5 ISO_SOAK_THINK_MS=0 $H perf soak > $R/warmup-$m.jsonl 2>&1; done; log warmup
for row in "abi 0" "abi-uu 0" "abi 100" "abi-uu 100" "native 0"; do set -- $row
  quiet; log "soak-start $1 $2"
  ISO_MODES=$1 ISO_SOAK_THINK_MS=$2 ISO_SOAK_SECS=120 $H perf soak >> $R/soak.jsonl 2>> $R/soak.err; log "soak-row $1 $2 rc=$?"
done
log DONE
