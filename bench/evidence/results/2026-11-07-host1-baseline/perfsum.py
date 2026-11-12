"""Summarizes perf-bench.csv (`perf stat -x, -r 5`, mm run pinned to CPU 2)."""
import collections, csv, sys
d = collections.defaultdict(dict)
for r in csv.DictReader(open(sys.argv[1])):
    d[r['program']][r['event']] = float(r['value'])
print('| program | instructions M | cycles M | IPC | branch misses / kI | L1i misses / kI | iTLB misses / kI | L1d misses / kI | frontend stall % |')
print('|---|---:|---:|---:|---:|---:|---:|---:|---:|')
for p, e in d.items():
    i, c = e['instructions'], e['cycles']
    print(f"| {p} | {i/1e6:,.0f} | {c/1e6:,.0f} | {i/c:.2f} | {e['branch-misses']/i*1e3:.2f} | "
          f"{e['L1-icache-load-misses']/i*1e3:.2f} | {e['iTLB-load-misses']/i*1e3:.3f} | "
          f"{e['L1-dcache-load-misses']/i*1e3:.2f} | {e['stalled-cycles-frontend']/c*100:.1f} |")
