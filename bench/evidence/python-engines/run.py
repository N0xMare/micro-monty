"""Runs the Python-engine comparison (docs/research/python-engines-compare.md).

    python3.14 bench/evidence/python-engines/run.py [--rounds N] [--only SECTION,...] [--out DIR]
    python3.14 bench/evidence/python-engines/run.py --summarize-only --out DIR

Sections: sizes, capability (corpus, own cases, codemode solutions; once,
they are deterministic), then per round: startup, mem, hostcall, workload,
bench, cli, polling, snapshot (mm), control; and once: hostile, isolation. Every host
measurement is one process of hosts/* (or bench/evidence/engines/hosts/mm for
mm) pinned with `taskset -c $CORE` (default 2), with the engine order rotated
by one per measurement, as bench/evidence/engines/run.py does. Each process
prints one JSON line; this script adds `section`, `round` and `slot`, appends
it to DIR/raw.jsonl, and writes DIR/summary.json and DIR/summary.md (medians
over every sample of every round). Workload calls are judged by
bench/evidence/engines/check.py (the real codemode checker).

Engines and inputs come from the README's /tmp/peng and /tmp/eng layout
(`PENG`, `ENG`); `build.sh` there builds everything.
"""

from __future__ import annotations

import argparse
import gzip
import json
import os
import re
import resource
import statistics
import subprocess
import sys
import time
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
JS = ROOT / 'bench/evidence/engines'
BENCH = ROOT / 'bench'
SOLUTIONS = ROOT / 'conformance/codemode/solutions/s'
ENG = Path(os.environ.get('ENG', '/tmp/eng'))
PENG = Path(os.environ.get('PENG', '/tmp/peng'))
CORE = os.environ.get('CORE', '2')
PYTHON = os.environ.get('PYTHON') or str(Path.home() / '.local/bin/python3.14')
sys.path.insert(0, str(HERE))
import capability  # noqa: E402

HOSTS = {
    'mm': ENG / 'target-mm/release/engines-mm',
    'micropython': ENG / 'target-micropython/release/engines-micropython',
    'pocketpy': ENG / 'target-pocketpy/release/engines-pocketpy',
}
# Monty runs the `gcpause` section only (hosts/monty).
GC_HOSTS = {**HOSTS, 'monty': ENG / 'target-monty/release/engines-monty'}
# gcpause: live one-item lists kept, one-item lists of garbage made beside
# them (enough for several collections in MicroPython's 256 MiB arena), and
# whether each garbage list is a self-cycle.
GC_LIVE = [0, 1_000_000, 2_000_000]
GC_GARBAGE = 20_000_000
GC_KINDS = ['acyclic', 'cyclic']
# --engines: the engines this invocation runs (default all); --reuse copies
# the others' records from an earlier results directory.
SEL = set(GC_HOSTS)
CLIS = {
    'mm': [str(ROOT / 'target/release/mm'), 'run'],
    'micropython': [str(PENG / 'bin/micropython-bin'), '-X', 'heapsize=256M'],
    'pocketpy': [str(PENG / 'bin/pocketpy')],
    'cpython': [PYTHON],
}
PROGRAMS = ['startup', 'fib', 'loop', 'dict', 'str', 'json_rows', 'closure', 'classes', 'generators',
            'exceptions', 'fmt', 'sort', 'interp']
TASKS = sorted(p.stem for p in (JS / 'fixtures').glob('*.json'))
CONTROL = ['loop', 'sort', 'string', 'sort-loop', 'string-loop', 'bigint', 'catch', 'catch-all']
FIXTURE = str(JS / 'fixtures/neg-fanout-40.json')
SNAP_POINTS = [('neg-fanout-40', 0), ('agg-avg-order-by-tier', 3), ('par-two-level-fanout', 1)]
ENV = {'CONTROL_LIMIT_S': '10', 'PENG': str(PENG)}
# Per-process limits: a host past its own limits must not take the orb with it.
AS_LIMIT = 8 << 30
BENCH_TIMEOUT = 900
# One run of bench/dict.py on MicroPython takes over 8 minutes (16-bit string
# hashes: README); it is measured once, apart (`dict-micropython`), not here.
SKIP = {('micropython', 'dict')}


def rotate(xs: list[str], k: int) -> list[str]:
    k %= len(xs)
    return xs[k:] + xs[:k]


def limit():
    resource.setrlimit(resource.RLIMIT_AS, (AS_LIMIT, AS_LIMIT))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


class Runner:
    def __init__(self, out: Path):
        self.out = out
        self.raw = out / 'raw.jsonl'
        self.calls = out / 'calls.jsonl'
        self.slot = 0

    def emit(self, rec: dict) -> None:
        with self.raw.open('a') as f:
            f.write(json.dumps(rec) + '\n')

    def host(self, engine: str, args: list[str], section: str, rnd: int, timeout: float = 600) -> dict:
        cmd = ['taskset', '-c', CORE, str(GC_HOSTS[engine]), *args]
        t = time.monotonic()
        try:
            p = subprocess.run(cmd, capture_output=True, text=True, env={**os.environ, **ENV}, timeout=timeout,
                               cwd=JS, preexec_fn=limit)
        except subprocess.TimeoutExpired:
            rec = {'engine': engine, 'error': 'timeout', 'timeout_s': timeout, 'args': args}
        else:
            lines = [ln for ln in p.stdout.splitlines() if ln.startswith('{"')]
            if p.returncode != 0 or not lines:
                rec = {'engine': engine, 'error': (p.stderr or p.stdout)[-400:], 'args': args,
                       'status': p.returncode}
            else:
                rec = json.loads(lines[-1])
                rec.setdefault('engine', engine)
        rec.update(engine=engine, section=section, round=rnd, slot=self.slot, wall_s=round(time.monotonic() - t, 3))
        self.slot += 1
        self.emit(rec)
        if 'error' in rec and section not in ('hostile', 'mem'):
            print(f'  {engine} {" ".join(args)}: ERROR {str(rec["error"])[:160]}', file=sys.stderr)
        return rec

    def cli(self, engine: str, file: Path, rnd: int, reps: int) -> None:
        expected, ns = None, []
        for i in range(reps + 1):
            t = time.perf_counter_ns()
            try:
                p = subprocess.run(['taskset', '-c', CORE, *CLIS[engine], str(file)], capture_output=True, text=True,
                                   timeout=BENCH_TIMEOUT, preexec_fn=limit, env={**os.environ, **ENV})
            except subprocess.TimeoutExpired:
                self.emit({'engine': engine, 'section': 'cli', 'file': file.name, 'round': rnd, 'error': 'timeout'})
                return
            dt = time.perf_counter_ns() - t
            if p.returncode != 0:
                self.emit({'engine': engine, 'section': 'cli', 'file': file.name, 'round': rnd,
                           'error': (p.stderr or p.stdout)[-300:]})
                return
            if i == 0:
                expected = p.stdout
            ns.append(dt)
        self.emit({'engine': engine, 'section': 'cli', 'cmd': 'cli', 'file': file.name, 'output': expected,
                   'ns': ns[1:] or ns, 'round': rnd, 'slot': self.slot})
        self.slot += 1


def de_underscore(src: str) -> str:
    """`3_000_000` -> `3000000`: PocketPy's lexer rejects underscores in
    numeric literals. Applied to the bench programs for the speed tables only
    (as `pocketpy*`), and reported."""
    return re.sub(r'(?<=\d)_(?=\d)', '', src)


def prepare(out: Path) -> None:
    """The per-engine inputs: bench programs (PocketPy's de-underscored) and
    codemode scripts (MicroPython's top-level await wrapped, PocketPy's async
    layer removed: capability.py)."""
    for name in PROGRAMS:
        d = out / 'inputs/pocketpy'
        d.mkdir(parents=True, exist_ok=True)
        (d / f'{name}.py').write_text(de_underscore((BENCH / f'{name}.py').read_text()))
    for task in TASKS:
        src = (SOLUTIONS / f'{task}.py').read_text()
        for engine, text in [('micropython', capability.wrap_top_level_await(src)),
                             ('pocketpy', capability.sync_rewrite(src))]:
            d = out / 'inputs' / engine
            d.mkdir(parents=True, exist_ok=True)
            (d / f'{task}.py').write_text(text)


def program(out: Path, name: str, engine: str) -> Path:
    return out / f'inputs/pocketpy/{name}.py' if engine == 'pocketpy' else BENCH / f'{name}.py'


def script(out: Path, task: str, engine: str) -> Path:
    return SOLUTIONS / f'{task}.py' if engine == 'mm' else out / f'inputs/{engine}/{task}.py'


def hostile_cases() -> dict[str, str]:
    """Hostile inputs: nesting, recursion, allocation, pathological regex,
    big ints, and the minimized `run_source` units of mm's last fuzz campaign
    (conformance/breadth/fuzz/artifacts; their first byte is the target's
    selector)."""
    deep = 'x = []\nfor _ in range(1000000):\n    x = [x]\n'
    cases = {
        'nest-parens-source': 'x = ' + '(' * 100000 + '1' + ')' * 100000 + '\n',
        'nest-list-source': 'x = ' + '[' * 100000 + ']' * 100000 + '\n',
        'nest-data-repr': deep + 'print(len(repr(x)))\n',
        'nest-data-eq': deep + 'y = []\nfor _ in range(1000000):\n    y = [y]\nprint(x == y)\n',
        'nest-data-json': deep + 'import json\nprint(len(json.dumps(x)))\n',
        'recursion': 'def f(n):\n    return f(n + 1)\nf(0)\n',
        'recursion-caught': 'def f(n):\n    return f(n + 1)\ntry:\n    f(0)\nexcept RecursionError:\n    print("caught")\n',
        'recursion-repr': 'class A:\n    def __repr__(self):\n        return repr(self)\nrepr(A())\n',
        'recursion-generator': 'def g():\n    yield from g()\nlist(g())\n',
        'alloc-str-1TiB': "x = 'a' * (1 << 40)\n",
        'alloc-list-1TiB': 'x = [0] * (1 << 40)\n',
        'alloc-grow': "xs = []\nwhile True:\n    xs.append('x' * 1000000)\n",
        'alloc-grow-caught': "xs = []\ntry:\n    while True:\n        xs.append('x' * 1000000)\nexcept MemoryError:\n    n = len(xs)\n    xs = None\n    print('caught', n > 0)\n",
        'alloc-format-1GB': "print(len('{:>1000000000}'.format(1)))\n",
        'regex-redos': "import re\nprint(re.match('(a+)+$', 'a' * 28 + 'b'))\n",
        'bigint-1e8-digits': 'x = 10 ** (10 ** 8)\nprint(x % 7)\n',
    }
    art = ROOT / 'conformance/breadth/fuzz/artifacts'
    for name in ['run_source-oom-asan-bigpow-11c5a481-min', 'run_source-slow-strindex-58d44444-min',
                 'run_source-timeout-excrender-34f25cd9-min']:
        cases['fuzz-' + name.split('-')[2]] = (art / name).read_bytes()[1:].decode('utf-8', 'replace')
    return cases


def sections(r: Runner, out: Path, rnd: int, only: set[str]) -> None:
    k = rnd
    want = lambda s: not only or s in only  # noqa: E731
    engines = [e for e in HOSTS if e in SEL]
    gc_engines = [e for e in GC_HOSTS if e in SEL]
    if want('gcpause') and gc_engines:
        for live in GC_LIVE:
            for kind in GC_KINDS:
                for e in rotate(gc_engines, k):
                    r.host(e, ['gcpause', str(live), str(GC_GARBAGE), kind], 'gcpause', rnd, timeout=600)
                k += 1
    if not engines:
        return
    if want('startup'):
        for e in rotate(engines, k):
            r.host(e, ['startup', '20', FIXTURE], 'startup', rnd)
        k += 1
    if want('mem'):
        for mode in ['empty', 'setup']:
            for n in [1, 8, 40]:
                for e in rotate(engines, k):
                    r.host(e, ['mem', mode, str(n), FIXTURE], 'mem', rnd)
                k += 1
        for n in [1, 8, 40] if 'micropython' in SEL else []:
            r.host('micropython', ['heap', str(n), FIXTURE], 'heap', rnd)
    if want('hostcall'):
        for kind, n, reps in [('sync', 10000, 5), ('await', 10000, 5), ('fanout', 100, 5)]:
            for e in rotate(engines, k):
                r.host(e, ['hostcall', kind, str(n), str(reps)], 'hostcall', rnd)
            k += 1
    if want('workload'):
        for task in TASKS:
            for e in rotate(engines, k):
                rec = r.host(e, ['workload', f'fixtures/{task}.json', str(script(out, task, e)), '5'], 'workload', rnd)
                if rnd == 0 and 'check' in rec:
                    with r.calls.open('a') as f:
                        f.write(json.dumps({'engine': e, 'task': task, 'calls': rec['check']['calls'],
                                            'error': rec['check']['error']}) + '\n')
            k += 1
    if want('bench'):
        for name in PROGRAMS:
            for e in rotate(engines, k):
                if (e, name) in SKIP:
                    r.emit({'engine': e, 'section': 'bench', 'file': f'{name}.py', 'round': rnd, 'error': 'skipped'})
                    continue
                r.host(e, ['bench', str(program(out, name, e)), '3'], 'bench', rnd, timeout=BENCH_TIMEOUT)
            k += 1
    if want('cli'):
        for name in PROGRAMS:
            for e in rotate([c for c in CLIS if c in SEL], k):
                if (e, name) in SKIP:
                    r.emit({'engine': e, 'section': 'cli', 'file': f'{name}.py', 'round': rnd, 'error': 'skipped'})
                    continue
                r.cli(e, program(out, name, e), rnd, 3)
            k += 1
    if want('polling'):
        for name in ['loop', 'fib', 'sort']:
            for e in rotate(engines, k):
                r.host(e, ['polling', str(program(out, name, e)), '3'], 'polling', rnd, timeout=BENCH_TIMEOUT)
            k += 1
    if want('snapshot') and 'mm' in SEL:
        # Only mm can snapshot a run (README "Hosts"): its mid-await images,
        # as in the JS study.
        for task, after in SNAP_POINTS:
            r.host('mm', ['snapshot', f'fixtures/{task}.json', str(SOLUTIONS / f'{task}.py'), str(after), '10'],
                   'snapshot', rnd)
    if want('control'):
        for kind in CONTROL:
            for e in rotate(engines, k):
                r.host(e, ['control', kind], 'control', rnd, timeout=60)
            k += 1


def once(r: Runner, out: Path, only: set[str]) -> None:
    want = lambda s: not only or s in only  # noqa: E731
    if want('hostile'):
        d = out / 'inputs/hostile'
        d.mkdir(parents=True, exist_ok=True)
        for name, src in hostile_cases().items():
            (d / f'{name}.py').write_text(src)
            for e in [e for e in HOSTS if e in SEL]:
                r.host(e, ['hostile', str(d / f'{name}.py')], 'hostile', 0, timeout=60)
    if want('dict-micropython') and 'micropython' in SEL:
        # The skipped program, once, as a CLI process.
        t = time.perf_counter_ns()
        p = subprocess.run(['taskset', '-c', CORE, *CLIS['micropython'], str(BENCH / 'dict.py')], capture_output=True,
                           text=True, preexec_fn=limit)
        r.emit({'engine': 'micropython', 'section': 'dict-micropython', 'ns': [time.perf_counter_ns() - t],
                'output': p.stdout, 'status': p.returncode})
    if want('isolation'):
        for e in [e for e in HOSTS if e in SEL]:
            r.host(e, ['isolation'], 'isolation', 0)


def reuse(src: Path, out: Path) -> None:
    """Copies the records of the engines this run leaves out (not in SEL) from
    an earlier results directory: their raw.jsonl lines (sections this
    directory has none of for that engine), their capability files and their
    workload calls. Their builds are pinned, so only the machine can move
    their numbers: reuse a directory measured on the same machine."""
    have = set()
    raw = out / 'raw.jsonl'
    if raw.exists():
        have = {(r.get('engine'), r.get('section')) for r in map(json.loads, raw.read_text().splitlines())}
    keep = [ln for ln in (src / 'raw.jsonl').read_text().splitlines()
            if (r := json.loads(ln)).get('engine') not in SEL and (r.get('engine'), r.get('section')) not in have]
    with raw.open('a') as f:
        f.writelines(ln + '\n' for ln in keep)
    for p in src.glob('*-*.jsonl'):
        e = p.stem.split('-', 1)[1].removesuffix('-sync')
        if p.stem.split('-', 1)[0] in ('corpus', 'codemode') and e not in SEL and not (out / p.name).exists():
            (out / p.name).write_text(p.read_text())
    if (src / 'calls.jsonl').exists():
        done = set()
        if (out / 'calls.jsonl').exists():
            done = {json.loads(ln)['engine'] for ln in (out / 'calls.jsonl').read_text().splitlines()}
        calls = [ln for ln in (src / 'calls.jsonl').read_text().splitlines()
                 if json.loads(ln)['engine'] not in SEL | done]
        with (out / 'calls.jsonl').open('a') as f:
            f.writelines(ln + '\n' for ln in calls)
    (out / 'reused-from.txt').write_text(f'{src.name}: every engine but {", ".join(sorted(SEL))}\n')
    print(f'reused {len(keep)} records from {src}', file=sys.stderr)


def sizes() -> list[dict]:
    t = ROOT / 'target'
    files = [
        ('mm', 'native', t / 'x86_64-unknown-linux-gnu/release/examples/hello', '`cargo xtask size` hello'),
        ('micropython', 'native', PENG / 'size/micropython-native', 'sandbox variant, upstream -Os, stripped'),
        ('micropython-standard', 'native', PENG / 'size/micropython-standard', 'unix standard variant, -Os, stripped'),
        ('pocketpy', 'native', PENG / 'size/pocketpy-native', 'pocketpy.c + hello, -Os -flto, gc-sections, stripped'),
        ('pocketpy -O2 host build', 'native', PENG / 'bin/pocketpy', 'upstream CLI, -O2 (speed build), not stripped'),
        ('mm', 'wasm32-unknown-unknown', t / 'wasm32-unknown-unknown/release/shipped/hello.wasm',
         '`cargo xtask size` hello.wasm'),
        ('mm-wasm guest', 'wasm32-unknown-unknown', t / 'wasm32-unknown-unknown/shipped/mm_wasm_guest.wasm',
         '`cargo xtask size`'),
        ('micropython', 'wasm32 (emscripten)', PENG / 'size/micropython.wasm',
         'npm @micropython/micropython-webassembly-pyscript 1.29.0-6 (+ 110 KB .mjs glue)'),
        ('pocketpy', 'wasm32-wasip1', PENG / 'size/pocketpy.wasm', 'wasi-sdk 34 -Oz -flto, wasm-opt -Oz'),
    ]
    out = []
    for engine, target, path, how in files:
        if path.exists():
            data = path.read_bytes()
            out.append({'engine': engine, 'target': target, 'bytes': len(data), 'gzip9': len(gzip.compress(data, 9)),
                        'how': how})
        else:
            out.append({'engine': engine, 'target': target, 'missing': str(path)})
    return out


# ---------------------------------------------------------------------------
# Summary


def med(xs):
    return statistics.median(xs) if xs else None


def summarize(out: Path) -> dict:
    recs = [json.loads(ln) for ln in (out / 'raw.jsonl').read_text().splitlines()] if (out / 'raw.jsonl').exists() else []
    s: dict = {'errors': [{k: r.get(k) for k in ('engine', 'section', 'args', 'error', 'file')}
                          for r in recs if 'error' in r and r.get('section') not in ('hostile',)]}
    ok = lambda r, e, sec: r.get('engine') == e and r.get('section') == sec and 'error' not in r  # noqa: E731
    flat = lambda sel, f: [x for r in sel for x in (r[f] if isinstance(r[f], list) else [r[f]])]  # noqa: E731
    engines = list(GC_HOSTS)
    for e in engines:
        for live in GC_LIVE:
            for kind in GC_KINDS:
                sel = [r for r in recs if r.get('engine') == e and r.get('section') == 'gcpause'
                       and r.get('live') == live and r.get('cyclic') == (kind == 'cyclic')]
                good = [r for r in sel if 'error' not in r]
                key = f'{live}-{kind}'
                if good:
                    g = lambda ph, f: med([r[ph][f] for r in good if r.get(ph)])  # noqa: E731
                    s.setdefault('gcpause', {}).setdefault(e, {})[key] = {
                        'steady_max_ms': g('steady', 'max_ms'), 'steady_p99_ms': g('steady', 'p99_ms'),
                        'steady_median_ms': g('steady', 'median_ms'), 'steady_over_10ms': g('steady', 'over_10ms'),
                        'build_max_ms': g('build', 'max_ms'), 'build_over_10ms': g('build', 'over_10ms'),
                        'drop_ms': med([r['drop_ms'] for r in good]), 'total_ms': med([r['total_ms'] for r in good]),
                        'rss_peak_mb': med([r['rss_peak'] for r in good]) / 2**20, 'n': len(good)}
                elif sel:
                    s.setdefault('gcpause', {}).setdefault(e, {})[key] = {
                        'error': str(sel[0]['error']).strip().splitlines()[-1][:100] if str(sel[0]['error']).strip()
                        else f"status {sel[0].get('status')}"}
        st = [r for r in recs if ok(r, e, 'startup')]
        if st:
            s.setdefault('startup', {})[e] = {f: med(flat(st, f)) for f in
                                              ['runtime_ns', 'session_ns', 'setup_ns', 'first_cell_ns']}
        for mode in ['empty', 'setup']:
            for n in [1, 8, 40]:
                sel = [r for r in recs if r.get('engine') == e and r.get('section') == 'mem'
                       and (r.get('mode'), r.get('n')) == (mode, n) or
                       (r.get('engine') == e and r.get('section') == 'mem' and 'error' in r
                        and r.get('args', [None, None, None])[1:3] == [mode, str(n)])]
                good = [r for r in sel if 'error' not in r]
                if good:
                    s.setdefault('mem', {}).setdefault(e, {})[f'{mode}-{n}'] = {
                        'rss_delta': med([r['rss_after'] - r['rss_before'] for r in good]),
                        'heap_delta': med([r['heap_after'] - r['heap_before'] for r in good]),
                        'rss_before': med([r['rss_before'] for r in good])}
                elif sel:
                    s.setdefault('mem', {}).setdefault(e, {})[f'{mode}-{n}'] = {'error': sel[0]['error'][-80:]}
        for kind in ['sync', 'await', 'fanout']:
            sel = [r for r in recs if ok(r, e, 'hostcall') and r['kind'] == kind]
            if sel:
                calls = sel[0]['n'] * (100 if kind == 'fanout' else 1)
                tool, local = med(flat(sel, 'tool_ns')), med(flat(sel, 'local_ns'))
                s.setdefault('hostcall', {}).setdefault(e, {})[kind] = {
                    'per_call_ns': (tool - local) / calls, 'per_call_total_ns': tool / calls}
            elif any(r.get('engine') == e and r.get('section') == 'hostcall' and (r.get('args') or ['', ''])[1] == kind
                     for r in recs):
                s.setdefault('hostcall', {}).setdefault(e, {})[kind] = {'unsupported': True}
        verdicts = {}
        if (out / 'check.txt').exists():
            for ln in (out / 'check.txt').read_text().splitlines():
                parts = ln.split(None, 2)
                if len(parts) == 3:
                    verdicts[(parts[0], parts[1])] = parts[2]
        for task in TASKS:
            sel = [r for r in recs if ok(r, e, 'workload') and r['task'] == task]
            # Only runs the real checker passes count (a failing script can
            # end early): the others are listed as failing.
            if sel and verdicts.get((e, task), 'pass') != 'pass':
                s.setdefault('workload_fail', {}).setdefault(e, {})[task] = verdicts[(e, task)][:120]
            elif sel:
                s.setdefault('workload', {}).setdefault(e, {})[task] = {
                    f: med(flat(sel, f)) for f in ['ns', 'setup_ns', 'run_ns']}
        for sec in ['bench', 'cli', 'polling']:
            for name in PROGRAMS:
                sel = [r for r in recs if r.get('engine') == e and r.get('section') == sec
                       and Path(r.get('file') or (r.get('args') or ['', ''])[1]).stem == name]
                good = [r for r in sel if 'error' not in r]
                if good and sec == 'polling':
                    p, q = med(flat(good, 'plain_ns')), med(flat(good, 'polled_ns'))
                    s.setdefault(sec, {}).setdefault(e, {})[name] = {'overhead': q / p - 1}
                elif good:
                    s.setdefault(sec, {}).setdefault(e, {})[name] = {'ns': med(flat(good, 'ns')),
                                                                     'output': good[0]['output'].strip()}
                elif sel:
                    s.setdefault(sec, {}).setdefault(e, {})[name] = {'error': str(sel[0]['error'])[-200:]}
        for kind in CONTROL:
            sel = [r for r in recs if r.get('engine') == e and r.get('section') == 'control' and r.get('kind') == kind]
            if sel:
                lat = [r['latency_ns'] for r in sel if isinstance(r.get('latency_ns'), (int, float))]
                s.setdefault('control', {}).setdefault(e, {})[kind] = {
                    'how': sorted({r.get('how', 'error') for r in sel}),
                    'resumable': sorted({str(r.get('resumable')) for r in sel}),
                    'session_usable': sorted({str(r.get('session_usable')) for r in sel}),
                    'latency_ns': med(lat), 'n': len(sel)}
        for r in recs:
            if r.get('engine') == e and r.get('section') == 'hostile':
                name = Path((r.get('file') or r['args'][1])).stem
                if 'error' in r and 'cmd' not in r:
                    st = r.get('status')
                    how = ('timeout (60 s)' if r['error'] == 'timeout' else
                           f'host died: signal {-st}' if isinstance(st, int) and st < 0 else
                           f'host exited {st}: {r["error"].strip().splitlines()[-1][:100] if r["error"].strip() else ""}')
                    s.setdefault('hostile', {}).setdefault(e, {})[name] = {'outcome': how, 'probe_ok': None}
                else:
                    s.setdefault('hostile', {}).setdefault(e, {})[name] = {
                        'outcome': 'ok: ' + (r.get('output') or '').strip()[:60] if r['ok'] else r['error'],
                        'ms': r['ms'], 'probe_ok': r['probe_ok'], 'rss_peak_mb': r['rss_peak'] / 2**20}
            if r.get('engine') == e and r.get('section') == 'isolation' and 'error' not in r:
                s.setdefault('isolation', {})[e] = {k: r.get(k) for k in ('other_session', 'same_session', 'isolated')}
        for task, after in SNAP_POINTS:
            sel = [r for r in recs if ok(r, e, 'snapshot') and r.get('task') == task]
            if sel:
                s.setdefault('snapshot', {}).setdefault(e, {})[f'{task}@{after}'] = {
                    'raw_bytes': sel[0]['raw_bytes'], 'zstd1_bytes': sel[0]['zstd1_bytes'],
                    'snapshot_ns': med(flat(sel, 'snapshot_ns')), 'restore_ns': med(flat(sel, 'restore_ns')),
                    'finished': all(r['finished']['ok'] and not r['finished']['misses'] for r in sel)}
        heap = [r for r in recs if r.get('engine') == e and r.get('section') == 'heap' and 'error' not in r]
        for n in [1, 8, 40]:
            sel = [r for r in heap if r['n'] == n]
            if sel:
                s.setdefault('heap', {}).setdefault(e, {})[n] = {'base': med([r['base'] for r in sel]),
                                                                  'used': med([r['used'] for r in sel])}
    return s


def capability_summary(out: Path) -> dict:
    res: dict = {}
    for e in ['mm', 'micropython', 'pocketpy']:
        p = out / f'corpus-{e}.jsonl'
        if p.exists():
            recs = [json.loads(ln) for ln in p.read_text().splitlines()]
            cp = [r for r in recs if 'cpython' in r['tags']]
            res.setdefault('corpus', {})[e] = {'cases': len(cp), 'status': dict(Counter(r['status'] for r in cp)),
                                               'causes': Counter(group(r['cause']) for r in cp if r['cause']).most_common()}
        p = out / f'own-{e}.jsonl'
        if p.exists():
            recs = [json.loads(ln) for ln in p.read_text().splitlines()]
            res.setdefault('own', {})[e] = {'cases': len(recs), 'status': dict(Counter(r['status'] for r in recs)),
                                            'strict': sum(r['strict'] for r in recs),
                                            'causes': Counter(group(r.get('cause')) for r in recs
                                                              if r['status'] != 'pass').most_common()}
    runs = [json.loads(ln) for ln in (ROOT / 'conformance/codemode/results/runs.jsonl').read_text().splitlines()]
    cp_pass = {(r['set'], r['task']) for r in runs if r['cpython']['outcome'] == 'pass'}
    res['codemode'] = {'cpython': {'solutions': len(runs), 'pass': len(cp_pass)},
                       'mm': {'pass': sum(1 for r in runs if (r['set'], r['task']) in cp_pass
                                          and r['mm-full']['outcome'] == 'pass'),
                              'causes': Counter(group(cm_cause(r['mm-full'])) for r in runs
                                                if (r['set'], r['task']) in cp_pass
                                                and r['mm-full']['outcome'] != 'pass').most_common()}}
    for e in ['micropython', 'pocketpy', 'pocketpy-sync']:
        p = out / f'codemode-{e}.jsonl'
        if p.exists():
            recs = [json.loads(ln) for ln in p.read_text().splitlines()]
            mine = [r for r in recs if (r['set'], r['task']) in cp_pass]
            res['codemode'][e] = {'pass': sum(r['outcome'] == 'pass' for r in mine),
                                  'causes': Counter(group(cm_cause(r)) for r in mine
                                                    if r['outcome'] != 'pass').most_common()}
    return res


def cm_cause(r: dict) -> str | None:
    if r['outcome'] == 'pass':
        return None
    if r['outcome'] == 'wrong':
        return 'wrong output'
    e = r.get('error') or {}
    return f"{e.get('type')}: {e.get('message', '')}"


def group(cause: str | None) -> str:
    """Failure causes in the doc's groups: syntax, missing module, missing
    builtin, missing attribute or method, exception message, traceback
    format, semantics (wrong result or exception), crash, timeout."""
    if not cause:
        return 'none'
    c = cause
    if c.startswith(('syntax', 'SyntaxError')):
        return 'syntax'
    if c.startswith('missing module') or c.startswith(('import ', 'ImportError', 'ModuleNotFoundError')):
        m = (re.search(r"named '([\w.]+)'", c) or re.search(r"^import ([\w.]+)", c)
             or re.search(r"module:? '?([\w.]+)", c))
        return f'missing module: {m.group(1)}' if m else 'missing module'
    if c.startswith('missing builtin') or (c.startswith('NameError') and re.search(r"'(\w+)'", c)
                                           and re.search(r"'(\w+)'", c).group(1) in capability.BUILTINS):
        return 'missing builtin'
    if c.startswith('missing attribute') or (c.startswith('AttributeError') and 'attribute' in c):
        return 'missing attribute or method'
    if c in ('crash', 'timeout', 'traceback format', 'exception message', 'not implemented'):
        return c
    if c.startswith('NotImplementedError'):
        return 'not implemented'
    if 'HarnessError' in c:
        return 'crash'
    return 'semantics'


ORDER = ['mm', 'micropython', 'pocketpy', 'cpython']


def markdown(s: dict, cap: dict, sizes_: list[dict]) -> str:
    def ms(ns, d=2):
        return '—' if ns is None else f'{ns / 1e6:,.{d}f}'

    def us(ns, d=1):
        return '—' if ns is None else f'{ns / 1e3:,.{d}f}'

    out = ['# Python engines: medians', '']
    out += ['## Capability', '']
    if 'corpus' in cap:
        out += ['Monty corpus, `cpython`-tagged cases (judged by conformance/oracle.py):', '',
                '| Engine | Cases | Pass | Fail | Crash | Top causes |', '|---|---:|---:|---:|---:|---|']
        for e, v in cap['corpus'].items():
            st = v['status']
            out.append(f"| {e} | {v['cases']} | {st.get('pass', 0)} | {st.get('fail', 0)} | {st.get('crash', 0)} | "
                       + ', '.join(f'{c} {n}' for c, n in v['causes'][:8]) + ' |')
    if 'own' in cap:
        out += ['', 'conformance/own as scripts vs CPython (stdout and exit status; strict adds stderr):', '',
                '| Engine | Cases | Pass | Strict | Top causes |', '|---|---:|---:|---:|---|']
        for e, v in cap['own'].items():
            out.append(f"| {e} | {v['cases']} | {v['status'].get('pass', 0)} | {v['strict']} | "
                       + ', '.join(f'{c} {n}' for c, n in v['causes'][:8]) + ' |')
    cm = cap.get('codemode', {})
    out += ['', f"Codemode solutions that pass on CPython ({cm.get('cpython', {}).get('pass')} of "
            f"{cm.get('cpython', {}).get('solutions')}):", '', '| Engine | Pass | Top causes |', '|---|---:|---|']
    for e, v in cm.items():
        if e != 'cpython':
            out.append(f"| {e} | {v['pass']} | " + ', '.join(f'{c} {n}' for c, n in v['causes'][:8]) + ' |')
    out += ['', '## Size', '', '| Engine | Target | Bytes | gzip -9 | Build |', '|---|---|---:|---:|---|']
    for x in sizes_:
        if 'bytes' in x:
            out.append(f"| {x['engine']} | {x['target']} | {x['bytes']:,} | {x['gzip9']:,} | {x['how']} |")
    out += ['', '## Start-up (µs)', '', '| Engine | New runtime | New session | Codemode setup | First cell |',
            '|---|---:|---:|---:|---:|']
    for e, d in s.get('startup', {}).items():
        out.append(f'| {e} | ' + ' | '.join(us(d[f]) for f in ['runtime_ns', 'session_ns', 'setup_ns', 'first_cell_ns'])
                   + ' |')
    out += ['', '## Memory (KB added by N live sessions: RSS / malloc heap)', '',
            '| Engine | ' + ' | '.join(f'{m} {n}' for m in ['empty', 'setup'] for n in [1, 8, 40]) + ' |',
            '|---|' + '---:|' * 6]
    for e, d in s.get('mem', {}).items():
        cells = []
        for m in ['empty', 'setup']:
            for n in [1, 8, 40]:
                v = d.get(f'{m}-{n}')
                cells.append('—' if v is None else 'n/a' if 'error' in v else
                             f"{v['rss_delta'] / 1e3:,.0f} / {v['heap_delta'] / 1e3:,.0f}")
        out.append(f'| {e} | ' + ' | '.join(cells) + ' |')
    for e, d in s.get('heap', {}).items():
        out.append('')
        out.append(f'{e} GC heap in use after N set-up sessions: ' +
                   ', '.join(f"{n}: {v['used']:,} B (base {v['base']:,})" for n, v in d.items()))
    out += ['', '## Host calls (µs per call: tool minus local, and total)', '',
            '| Engine | Sync | Sync total | Awaited | Awaited total | Fan-out | Fan-out total |',
            '|---|---:|---:|---:|---:|---:|---:|']
    for e, d in s.get('hostcall', {}).items():
        cells = []
        for k in ['sync', 'await', 'fanout']:
            v = d.get(k)
            cells += ['—', '—'] if v is None or v.get('unsupported') else [us(v['per_call_ns'], 2),
                                                                           us(v['per_call_total_ns'], 2)]
        out.append(f'| {e} | ' + ' | '.join(cells) + ' |')
    wl = s.get('workload', {})
    out += ['', '## Codemode workload (ms per task: new session + setup + run; run alone in parentheses)', '',
            '| Task | ' + ' | '.join(wl) + ' |', '|---|' + '---:|' * len(wl)]
    for t in TASKS:
        out.append(f'| {t} | ' + ' | '.join(f"{ms(wl[e][t]['ns'])} ({ms(wl[e][t]['run_ns'])})" if t in wl[e]
                                            else 'fails' for e in wl) + ' |')
    for f, label in [('ns', 'sum, total'), ('run_ns', 'sum, run')]:
        out.append(f'| **{label}** | ' + ' | '.join(ms(sum(v[f] for v in wl[e].values()), 1) for e in wl) + ' |')
    out.append(f'| **tasks that ran** | ' + ' | '.join(str(len(wl[e])) for e in wl) + ' |')
    for sec, title in [('bench', 'Bench programs in-process (ms per run)'),
                       ('cli', 'Bench programs as CLI processes (ms, spawn to exit)')]:
        d = s.get(sec, {})
        es = [e for e in ORDER if e in d]
        out += ['', f'## {title}', '', '| Program | ' + ' | '.join(es) + ' |', '|---|' + '---:|' * len(es)]
        for p in PROGRAMS:
            cells = []
            for e in es:
                v = d[e].get(p)
                cells.append('—' if v is None else 'fails' if 'error' in v else ms(v['ns'], 1))
            out.append(f'| {p} | ' + ' | '.join(cells) + ' |')
    out += ['', '## Polling overhead (stop armed vs not)', '', '| Engine | loop | fib | sort |', '|---|---:|---:|---:|']
    for e, d in s.get('polling', {}).items():
        out.append(f'| {e} | ' + ' | '.join(f"{d[p]['overhead'] * 100:+.1f}%" if p in d and 'overhead' in d[p]
                                            else '—' for p in ['loop', 'fib', 'sort']) + ' |')
    out += ['', '## Control (ms to regain control; outcome)', '', '| Engine | ' + ' | '.join(CONTROL) + ' |',
            '|---|' + '---|' * len(CONTROL)]
    for e, d in s.get('control', {}).items():
        cells = []
        for k in CONTROL:
            v = d.get(k)
            if v is None:
                cells.append('—')
            elif 'not regained' in v['how']:
                cells.append(f"never (10 s), n={v['n']}")
            else:
                cells.append(f"{ms(v['latency_ns'])} ms, {'/'.join(v['how'])}, resumable {'/'.join(v['resumable'])}, "
                             f"usable {'/'.join(v['session_usable'])}, n={v['n']}")
        out.append(f'| {e} | ' + ' | '.join(cells) + ' |')
    gp = s.get('gcpause', {})
    if gp:
        out += ['', f'## GC pauses (ms; {GC_GARBAGE:,} one-item lists of garbage beside N live ones, `hsync` every 1,000)',
                '', 'Steady: longest gap / p99 gap / median gap while making garbage, and how many gaps passed 10 ms. '
                'Build: longest gap while the live set grew. Drop: `live = None`. Total: the whole script.', '',
                '| Engine | Live | Garbage | Steady max | p99 | median | >10 ms | Build max | Drop | Total | Peak RSS MB |',
                '|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|']
        f = lambda x, d=2: '—' if x is None else f'{x:,.{d}f}'  # noqa: E731
        for e, d in gp.items():
            for key, v in d.items():
                live, kind = key.split('-')
                if 'error' in v:
                    out.append(f"| {e} | {int(live):,} | {kind} | fails: {v['error']} |  |  |  |  |  |  |  |")
                    continue
                out.append(f"| {e} | {int(live):,} | {kind} | {f(v['steady_max_ms'])} | {f(v['steady_p99_ms'])} | "
                           f"{f(v['steady_median_ms'], 3)} | {f(v['steady_over_10ms'], 0)} | {f(v['build_max_ms'])} | "
                           f"{f(v['drop_ms'])} | {f(v['total_ms'], 0)} | {f(v['rss_peak_mb'], 0)} |")
    out += ['', '## Snapshots mid-await (mm only)', '',
            '| Point | Raw bytes | zstd-1 bytes | Snapshot µs | Restore µs | Restored run finished |', '|---|---:|---:|---:|---:|---|']
    for point, v in s.get('snapshot', {}).get('mm', {}).items():
        out.append(f"| {point} | {v['raw_bytes']:,} | {v['zstd1_bytes']:,} | {us(v['snapshot_ns'])} | "
                   f"{us(v['restore_ns'])} | {v['finished']} |")
    wf = s.get('workload_fail', {})
    if wf:
        out += ['', 'Workload tasks the checker failed (not timed above):', '']
        out += [f'- {e} `{t}`: {why}' for e, d in wf.items() for t, why in d.items()]
    out += ['', '## Hostile inputs (outcome; ms; same session usable after)', '',
            '| Case | ' + ' | '.join(s.get('hostile', {})) + ' |', '|---|' + '---|' * len(s.get('hostile', {}))]
    names = sorted({n for d in s.get('hostile', {}).values() for n in d})
    for n in names:
        cells = []
        for e, d in s['hostile'].items():
            v = d.get(n, {})
            extra = '' if v.get('ms') is None else f"; {v['ms']:,.0f} ms; {'usable' if v['probe_ok'] else 'NOT usable'}"
            cells.append(f"{v.get('outcome', '—')}{extra}".replace('|', '\\|'))
        out.append(f'| {n} | ' + ' | '.join(cells) + ' |')
    out += ['', '## Isolation (session B after session A patched builtins.len, json.dumps, asyncio.gather)', '',
            '| Engine | Session B sees | Session A sees | Isolated |', '|---|---|---|---|']
    for e, v in s.get('isolation', {}).items():
        out.append(f"| {e} | `{v['other_session']}` | `{v['same_session']}` | {v['isolated']} |")
    if s.get('errors'):
        seen = dict.fromkeys(json.dumps(x)[:300] for x in s['errors'])
        out += ['', '## Errors (host processes that failed, each listed once)', ''] + [f'- {x}' for x in seen]
    return '\n'.join(out) + '\n'


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--rounds', type=int, default=5)
    ap.add_argument('--only', default='')
    ap.add_argument('--out', default=str(HERE / 'results' / time.strftime('%Y-%m-%d')))
    ap.add_argument('--summarize-only', action='store_true')
    ap.add_argument('--engines', default=','.join(GC_HOSTS), help='engines to run (default all)')
    ap.add_argument('--reuse', help="earlier results directory: copy the other engines' records from it")
    a = ap.parse_args()
    out = Path(a.out).resolve()  # the hosts run in bench/evidence/engines
    out.mkdir(parents=True, exist_ok=True)
    SEL.clear()
    SEL.update(x for x in a.engines.split(',') if x)
    if a.reuse and not a.summarize_only:
        reuse(Path(a.reuse), out)
    only = {x for x in a.only.split(',') if x}
    want = lambda s: not only or s in only  # noqa: E731
    if not a.summarize_only:
        r = Runner(out)
        if want('sizes'):
            (out / 'sizes.json').write_text(json.dumps(sizes(), indent=1) + '\n')
        if want('capability'):
            for what, engines in [('corpus', 'micropython,pocketpy,mm'), ('own', 'micropython,pocketpy,mm'),
                                  ('codemode', 'micropython,pocketpy,pocketpy-sync')]:
                # conformance/own grows with mm, so every engine reruns it;
                # the corpus and codemode inputs are fixed.
                if what != 'own':
                    engines = ','.join(x for x in engines.split(',') if x.removesuffix('-sync') in SEL)
                if engines:
                    subprocess.run([PYTHON, str(HERE / 'capability.py'), what, '--engines', engines, '--out',
                                    str(out)], check=True)
        prepare(out)
        for e in [e for e in GC_HOSTS if e in SEL]:
            r.host(e, ['describe'], 'describe', -1)
        for rnd in range(a.rounds):
            print(f'round {rnd + 1}/{a.rounds}', file=sys.stderr, flush=True)
            sections(r, out, rnd, only)
        once(r, out, only)
        if r.calls.exists():
            p = subprocess.run([PYTHON, str(JS / 'check.py'), str(r.calls)], capture_output=True, text=True)
            (out / 'check.txt').write_text(p.stdout + p.stderr)
    summary = summarize(out)
    cap = capability_summary(out)
    (out / 'summary.json').write_text(json.dumps({'hosts': summary, 'capability': cap}, indent=1, sort_keys=True) + '\n')
    sizes_ = json.loads((out / 'sizes.json').read_text()) if (out / 'sizes.json').exists() else []
    (out / 'summary.md').write_text(markdown(summary, cap, sizes_))
    return 0


if __name__ == '__main__':
    sys.exit(main())
