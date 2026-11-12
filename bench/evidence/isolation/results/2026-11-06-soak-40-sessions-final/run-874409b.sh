cd <checkout>
while pgrep -f 'xtask gc-stress' > /dev/null; do sleep 15; done
quiet() { q=0; while [ $q -lt 3 ]; do if pgrep -x rustc >/dev/null || pgrep -x cargo >/dev/null; then q=0; else q=$((q+1)); fi; sleep 10; done; }
O=.amp/in/soak874; H=target/isolation/release/iso-harness; export ISO_ROOT=$PWD ISO_SOAK_SESSIONS=40 ISO_SOAK_SECS=300
{ git rev-parse HEAD; rustc --version; uname -a; sw_vers; shasum -a 256 $H target/isolation/wasm32-wasip1/release/mm_wasm_guest.wasm target/isolation/wasm32-unknown-unknown/release/mm_wasm_guest.wasm; uptime; } > $O/build.txt
for row in "native 0" "abi 0" "abi-uu 0" "native 100" "abi 100"; do set -- $row
  quiet
  echo "start $1 $2 $(date -u +%FT%TZ)" >> $O/progress.txt
  ISO_MODES=$1 ISO_SOAK_THINK_MS=$2 $H perf soak >> $O/soak.jsonl 2>> $O/soak.err; echo "row $1 $2 rc=$? $(date -u +%FT%TZ)" >> $O/progress.txt
done
echo DONE >> $O/progress.txt
