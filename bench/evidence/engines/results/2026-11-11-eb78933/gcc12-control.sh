#!/usr/bin/env bash
# Control: the QuickJS hosts built with gcc-12 against gcc-14 (bench only, in-process).
set -euo pipefail
cd /home/user/workspace/repo/bench/evidence/engines
for h in rquickjs dquickts; do
  (cd hosts/$h && CC=gcc-12 CARGO_TARGET_DIR=/tmp/eng/target-$h-gcc12 cargo build --release) > /tmp/eng/logs/gcc12-$h.log 2>&1
done
out=/tmp/eng/scratch/gcc12.jsonl; : > $out
for rnd in 1 2 3; do
  for p in fib loop dict str json_rows closure classes generators exceptions fmt sort interp; do
    for h in rquickjs dquickts; do
      for cc in 14 12; do
        bin=/tmp/eng/target-$h/release/engines-$h; [ $cc = 12 ] && bin=/tmp/eng/target-$h-gcc12/release/engines-$h
        taskset -c 2 $bin bench ../../$p.js 3 | python3 -c "import json,sys; r=json.loads(sys.stdin.read()); r.update(cc=$cc, round=$rnd); print(json.dumps({k: r[k] for k in ('engine','file','ns','cc','round')}))" >> $out
      done
    done
  done
done
python3 - <<'PY'
import json, statistics, math
rs = [json.loads(l) for l in open('/tmp/eng/scratch/gcc12.jsonl')]
for e in ['rquickjs', 'dquickts']:
    ratios = []
    for p in sorted({r['file'] for r in rs}):
        m = {cc: statistics.median([x for r in rs if r['engine'] == e and r['file'] == p and r['cc'] == cc for x in r['ns']]) for cc in (12, 14)}
        ratios.append(m[14] / m[12])
        print(e, p.split('/')[-1], f'{m[12]/1e6:.1f} {m[14]/1e6:.1f} {m[14]/m[12]:.3f}')
    print(e, 'geomean gcc14/gcc12', f'{math.exp(sum(map(math.log, ratios)) / len(ratios)):.3f}')
PY
