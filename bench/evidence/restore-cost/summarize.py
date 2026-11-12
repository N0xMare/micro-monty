"""Summarizes run.sh's passes: per target and image, the range over passes of
the best snapshot, the best restore and the first restore, in ms per MiB of
image, and the restore range in ms. Usage: summarize.py DIR (files TARGET.N).
"""
import sys
from collections import defaultdict
from pathlib import Path

rows = defaultdict(list)  # (target, label) -> [(mib, snap, best, first)]
parked = defaultdict(list)  # target -> ["lowest ..."]
order = []
for path in sorted(Path(sys.argv[1]).iterdir()):
    target = path.name.split('.')[0]
    for line in path.read_text().splitlines():
        label, *rest = line.split('\t')
        if label.startswith('dumps parked'):
            parked[target].append(f'{label}: {rest[0]}')
            continue
        if label not in order:
            order.append(label)
        rows[target, label].append(tuple(float(x) for x in rest))


def span(xs, fmt='{:.2f}'):
    lo, hi = min(xs), max(xs)
    return fmt.format(lo) if fmt.format(lo) == fmt.format(hi) else f'{fmt.format(lo)}–{fmt.format(hi)}'


for target in ('native', 'wasmtime', 'v8'):
    if not any(t == target for t, _ in rows):
        continue
    print(f'\n{target} (ms per MiB of image; range over passes)\n')
    print('| image | MiB | snapshot | restore, best of 5 | restore ms | first restore |')
    print('|---|---:|---:|---:|---:|---:|')
    for label in order:
        runs = rows[target, label]
        mib = runs[0][0]
        print(
            f'| {label} | {mib:.1f} | {span([r[1] / mib for r in runs])} '
            f'| {span([r[2] / mib for r in runs])} | {span([r[2] for r in runs], "{:.0f}")} '
            f'| {span([r[3] / mib for r in runs])} |'
        )
    for line in parked[target]:
        print(f'\n{line}', end='')
    print()
