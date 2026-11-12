cd <checkout>
O=.amp/in/soakb05; H=target/isolation/release/iso-harness; export ISO_ROOT=$PWD ISO_SOAK_SESSIONS=40
quiet() { q=0; while [ $q -lt 3 ]; do if pgrep -x rustc >/dev/null || pgrep -x cargo >/dev/null; then q=0; else q=$((q+1)); fi; sleep 10; done; }
until grep -q built $O/../build-b05.log 2>/dev/null; do sleep 5; done
{ git rev-parse HEAD; rustc --version; sw_vers; shasum -a 256 $H target/isolation/wasm32-*/release/mm_wasm_guest.wasm; uptime; } > $O/build.txt
# Warm-up: rebuild the .cwasm caches under the new file names (not measured).
for m in abi abi-uu; do ISO_MODES=$m ISO_SOAK_SECS=5 ISO_SOAK_THINK_MS=0 $H perf soak > $O/warmup-$m.jsonl 2>&1; done
echo "warmup done $(date -u +%FT%TZ)" >> $O/progress.txt
for row in "abi 0" "abi-uu 0" "abi 100" "abi-uu 100" "native 0"; do set -- $row
  quiet; echo "start $1 $2 $(date -u +%FT%TZ)" >> $O/progress.txt
  ISO_MODES=$1 ISO_SOAK_THINK_MS=$2 ISO_SOAK_SECS=120 $H perf soak >> $O/soak.jsonl 2>> $O/soak.err; echo "row $1 $2 rc=$? $(date -u +%FT%TZ)" >> $O/progress.txt
done
echo DONE >> $O/progress.txt
