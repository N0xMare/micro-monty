#!/usr/bin/env python3
"""Randomized comparison of aware datetime paths, micro-monty against CPython.

Runs datetime_aware_case.py with each seed on CPython (TZ=UTC) and on
`mm run`, and requires identical output: comparisons of aware/naive mixes,
fold, zoneinfo transitions and Python tzinfo subclasses, arithmetic,
utcoffset, isoformat and astimezone. Hashes are checked for consistency with
== inside each engine, not against CPython's values.

    python3.14 conformance/fuzz/datetime_aware.py [--mm target/release/mm] [--seeds 1,2,3]
"""
import argparse
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--mm', default=os.environ.get('MM', os.path.join(ROOT, 'target/release/mm')))
    ap.add_argument('--python', default=os.environ.get('PYTHON', 'python3.14'))
    ap.add_argument('--seeds', default='1,2,3')
    args = ap.parse_args()
    src = open(os.path.join(HERE, 'datetime_aware_case.py')).read()
    env = dict(os.environ, TZ='UTC')
    bad = 0
    with tempfile.TemporaryDirectory() as tmp:
        for seed in args.seeds.split(','):
            path = os.path.join(tmp, f'case_{seed}.py')
            with open(path, 'w') as f:
                f.write(src.replace('\nSEED = 1\n', f'\nSEED = {int(seed)}\n'))
            cp = subprocess.run([args.python, path], capture_output=True, text=True, env=env)
            mm = subprocess.run([args.mm, 'run', path], capture_output=True, text=True, env=env)
            a, b = cp.stdout.splitlines(), mm.stdout.splitlines()
            diff = [(x, y) for x, y in zip(a, b) if x != y]
            ok = cp.returncode == 0 and mm.returncode == 0 and len(a) == len(b) and not diff
            print(f'seed {seed}: {len(a)} cases, {len(diff)} differ, exit {cp.returncode}/{mm.returncode}'
                  + ('' if ok else ' FAIL'))
            for x, y in diff[:3]:
                print(f'  cpython: {x}\n  mm:      {y}')
            if mm.returncode:
                print(mm.stderr[-2000:])
            bad += not ok
    sys.exit(1 if bad else 0)


if __name__ == '__main__':
    main()
