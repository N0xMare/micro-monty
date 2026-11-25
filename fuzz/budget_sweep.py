#!/usr/bin/env python3
"""Refuse one engine memory-budget charge per child, over conformance/own.

Each case runs once unarmed (its baseline, which counts its charges), then
once per selected charge index N with the Nth charge refused, each run in a
fresh `budget_sweep` process followed by a canary cell in the same session.
See fuzz/README.md for the outcome classes. Exits 1 on any failure.
"""

import argparse
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
from time import monotonic


ROOT = Path(__file__).resolve().parent.parent


def run(binary, case, index, timeout):
    """(charges or None, outcome, detail, printed output)."""
    try:
        child = subprocess.run([binary, case, str(index)], capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return None, 'hang', f'over {timeout:g}s', b''
    if child.returncode:
        detail = ' '.join(child.stderr.decode(errors='replace').split())[-300:]
        return None, 'abort', f'exit={child.returncode} {detail}', b''
    header, _, output = child.stdout.partition(b'\n')
    try:
        count, kind, detail = header.decode().split('\t')
        return int(count), kind, detail, output
    except ValueError:
        return None, 'abort', f'bad report {header[:200]!r}', b''


def indices(charges, per_case):
    """Every index up to `per_case` charges, else `per_case` spread evenly."""
    if charges <= per_case:
        return range(1, charges + 1)
    if per_case == 1:
        return [1]
    return sorted({1 + (charges - 1) * i // (per_case - 1) for i in range(per_case)})


def first_difference(output, expected):
    offset = next((i for i, (got, want) in enumerate(zip(output, expected)) if got != want),
                  min(len(output), len(expected)))
    line = output.rfind(b'\n', 0, offset) + 1
    return output[line:line + 160], expected[expected.rfind(b'\n', 0, offset) + 1:][:160]


def classify(base, seen, kind, detail, output, index):
    """(class, detail). Classes starting `failure:` fail the sweep; `review:`
    is a changed result to judge by hand; the rest are the expected ones."""
    base_kind, base_detail, expected = base
    if kind in ('abort', 'hang', 'fuel'):
        return 'failure: abort or hang', f'{kind} {detail}'
    if kind == 'stuck':
        return 'failure: session cannot continue', detail
    if seen < index:
        return 'failure: missed injection', f'only {seen} charges'
    if kind == 'caught':
        return 'caught MemoryError (control)', ''
    if kind == 'exception' and detail == 'MemoryError':
        return 'raised MemoryError', ''
    if (kind, detail) != (base_kind, base_detail):
        return 'review: outcome changed', f'{kind} {detail}, baseline {base_kind} {base_detail}'
    if output == expected:
        return 'same as baseline', ''
    got, want = first_difference(output, expected)
    text = f'got {got!r}; baseline {want!r}'
    if output.count(b'MemoryError') > expected.count(b'MemoryError'):
        return 'printed MemoryError', text
    return 'review: output changed', text


def sweep(binary, case, args):
    counts = Counter()
    notes = defaultdict(list)
    name = Path(case).name
    charges, kind, detail, output = run(binary, case, 0, args.timeout)
    counts['baselines'] += 1
    if kind == 'compile':
        counts['compile rejections'] += 1
        return counts, notes
    if charges is None or kind not in ('complete', 'exception', 'caught'):
        counts['failure: baseline'] += 1
        notes[('failure: baseline', f'{kind} {detail}')].append(0)
        return counts, notes
    base = (kind, detail, output)
    counts['charges'] += charges
    for n in indices(charges, args.per_case):
        seen, kind, detail, output = run(binary, case, n, args.timeout)
        counts['injection runs'] += 1
        verdict, text = classify(base, seen if seen is not None else n, kind, detail, output, n)
        counts[verdict] += 1
        if verdict.startswith(('failure', 'review', 'printed')):
            notes[(verdict, f'{name}: {text}')].append(n)
    return counts, notes


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--per-case', type=int, default=256,
                        help='charge indices refused per case, spread over its charges')
    parser.add_argument('--jobs', type=int, default=os.cpu_count() or 1)
    parser.add_argument('--timeout', type=float, default=60, help='wall seconds per child')
    parser.add_argument('--filter', default='', help='case name substring (--filter=--catch: the control)')
    args = parser.parse_args()
    if args.per_case < 1 or args.jobs < 1 or args.timeout <= 0:
        parser.error('need per-case >= 1, jobs >= 1, timeout > 0')
    cases = sorted(str(p) for p in (ROOT / 'conformance/own').glob('*.py') if args.filter in p.name)
    if args.filter in '--catch':
        cases.append('--catch')
    if not cases:
        parser.error('no matching cases')
    # A crash must not leave a core file for every injected charge.
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    cargo = ['cargo', 'build', '--release', '--locked', '--manifest-path', 'fuzz/Cargo.toml',
             '--features', 'budget-sweep', '--bin', 'budget_sweep', '--message-format=json']
    build = subprocess.run(cargo, cwd=ROOT, stdout=subprocess.PIPE, text=True, check=True)
    binary = next(entry['executable'] for line in build.stdout.splitlines()
                  if (entry := json.loads(line)).get('executable'))
    started = monotonic()
    totals = Counter()
    notes = defaultdict(list)
    with ThreadPoolExecutor(args.jobs) as pool:
        for counts, found in pool.map(lambda case: sweep(binary, case, args), cases):
            totals.update(counts)
            for key, at in found.items():
                notes[key].extend(at)
    if '--catch' in cases and not totals['caught MemoryError (control)']:
        totals['failure: control never caught MemoryError'] += 1
    # One line per distinct result, with the indices that gave it.
    for (verdict, text), at in sorted(notes.items()):
        shown = ','.join(map(str, at[:8])) + (',...' if len(at) > 8 else '')
        print(f'{verdict} [{len(at)}x, N={shown}] {text}')
    print(f'{len(cases)} cases; up to {args.per_case} indices each; '
          f'{monotonic() - started:.1f}s', file=sys.stderr)
    for key, count in sorted(totals.items()):
        print(f'{count:9} {key}', file=sys.stderr)
    return int(any(key.startswith('failure:') and n for key, n in totals.items()))


if __name__ == '__main__':
    sys.exit(main())
