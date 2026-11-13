"""Native time per fuel unit of the bench programs (docs/user-guide.md, "Embedding guarantees").

    python3.14 -I hosts/js/bench/native_fuel.py OUT.json

Uses target/release/mm (`cargo build --release -p mm-cli`): the fuel from
`mm exec --json --report-fuel`, the time as the median of 7 `mm run` wall
times less an empty program's (bench/startup.py), pinned to one core ($CORE, 2).
"""
import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MM = Path(os.environ.get('MM', ROOT / 'target/release/mm'))
CORE = os.environ.get('CORE', '2')
PROGRAMS = ['classes', 'closure', 'dict', 'exceptions', 'fib', 'fmt', 'generators', 'interp',
            'json_rows', 'loop', 'sort', 'str']


def timed(args):
    t = time.perf_counter()
    p = subprocess.run(['taskset', '-c', CORE, str(MM), *args], capture_output=True, text=True, check=True)
    return (time.perf_counter() - t) * 1000, p.stdout


def main():
    base = statistics.median(timed(['run', str(ROOT / 'bench/startup.py')])[0] for _ in range(11))
    out = {'startup_ms': base, 'programs': {}}
    for name in PROGRAMS:
        f = str(ROOT / 'bench' / f'{name}.py')
        fuel = json.loads(timed(['exec', '--json', '--report-fuel', f])[1])['fuel']
        ms = statistics.median(timed(['run', f])[0] for _ in range(7)) - base
        out['programs']['json' if name == 'json_rows' else name] = {'ms': ms, 'fuel': fuel, 'us_per_fuel': ms * 1000 / fuel}
    Path(sys.argv[1]).write_text(json.dumps(out, indent=1) + '\n')


if __name__ == '__main__':
    main()
