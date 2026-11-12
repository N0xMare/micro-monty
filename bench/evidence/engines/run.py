"""Runs the engine comparison (docs/research/js-engines-compare.md).

    python3.14 bench/evidence/engines/run.py [--rounds N] [--only SECTION,...] [--out DIR] [--no-setup]

One command: it first runs setup.sh (fetches every engine at its pin and
builds mm, the CLIs and the hosts; skip with --no-setup), then the campaign,
then DIR/summary.md, whose "Headline" section holds the figures the doc's
summary table quotes. DIR defaults to results/<date>-<commit>.

Every measurement is one host process (hosts/*, README "Hosts") pinned to one
core with `taskset -c $CORE` (default 2), run once per round; within a round
the engine order rotates by one per measurement, so no engine always runs
first or after the same neighbour. Each process emits one JSON line; this
script adds `round`, `slot` (position in the round) and `section`, appends it
to DIR/raw.jsonl, and writes DIR/summary.json and DIR/summary.md with medians
over every sample of every round. Workload calls are judged by check.py (the
real codemode checker) on the first round.

Paths of the engines come from the environment (README "Pins"); the defaults
are the /tmp/eng layout the README builds.
"""

from __future__ import annotations

import argparse
import gzip
import json
import math
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
BENCH = ROOT / 'bench'
SOLUTIONS = ROOT / 'conformance' / 'codemode' / 'solutions' / 's'
ENG = Path(os.environ.get('ENG', '/tmp/eng'))
CORE = os.environ.get('CORE', '2')
PYTHON = os.environ.get('PYTHON') or str(Path.home() / '.local/bin/python3.14')

HOSTS = {
    'mm': ENG / 'target-mm/release/engines-mm',
    'mm-wasm': ENG / 'target-mm-wasm/release/engines-mm-wasm',
    'rquickjs': ENG / 'target-rquickjs/release/engines-rquickjs',
    'dquickts': ENG / 'target-dquickts/release/engines-dquickts',
    'djs': ENG / 'target-djs/release/engines-djs',
    'quickjs-wasi': ENG / 'target-mm-wasm/release/engines-quickjs-wasi',
    'djs-wasm': ENG / 'target-mm-wasm/release/djs-wasm',
}
PY = {'mm', 'mm-wasm'}
# djs-wasm runs plain programs, a new VM and pi's prelude only (its host's doc).
LIMITED = {'djs-wasm'}
CODEMODE = [e for e in HOSTS if e not in LIMITED]
SNAPSHOTS = ['mm', 'mm-wasm', 'dquickts', 'djs', 'quickjs-wasi']
CLIS = {
    'qjs-ng': ([str(ENG / 'bin/qjs-ng')], 'js'),
    'qjs-bellard': ([str(ENG / 'bin/qjs-bellard')], 'js'),
    'djs-cli': ([str(ENG / 'bin/djs'), 'run'], 'js'),
    'mm-cli': ([str(ROOT / 'target/release/mm'), 'run'], 'py'),
}
ENV = {
    'MM_WASM_GUEST': str(ROOT / 'target/wasm32-unknown-unknown/shipped/mm_wasm_guest.wasm'),
    'QUICKJS_WASM': str(ENG / 'qwasi/quickjs.wasm'),
    'DJS_WASM': str(ENG / 'wasm/djs_wasm.wasm'),
    'CONTROL_LIMIT_S': '10',
}
PROGRAMS = ['startup', 'fib', 'loop', 'dict', 'str', 'json_rows', 'closure', 'classes', 'generators',
            'exceptions', 'fmt', 'sort', 'interp']
TASKS = sorted(p.stem for p in (HERE / 'fixtures').glob('*.json'))
# The `gc` section: live pairs (about 59 and 199 MiB in mm, spec §9.2's
# rows), a 1 GiB limit.
GC_ENGINES = ['mm', 'rquickjs', 'dquickts', 'djs']
GC_ITEMS = [550_000, 2_000_000]
CONTROL = ['loop', 'sort', 'string', 'sort-loop', 'string-loop']
SNAP_POINTS = [('neg-fanout-40', 0), ('agg-avg-order-by-tier', 3), ('par-two-level-fanout', 1)]


def rotate(xs: list[str], k: int) -> list[str]:
    k %= len(xs)
    return xs[k:] + xs[:k]


class Runner:
    def __init__(self, out: Path):
        self.out = out
        self.raw = out / 'raw.jsonl'
        self.calls = out / 'calls.jsonl'
        self.slot = 0

    def emit(self, rec: dict) -> None:
        with self.raw.open('a') as f:
            f.write(json.dumps(rec) + '\n')

    def host(self, engine: str, args: list[str], section: str, rnd: int, timeout: float = 600) -> dict | None:
        cmd = ['taskset', '-c', CORE, str(HOSTS[engine]), *args]
        env = {**os.environ, **ENV}
        t = time.monotonic()
        try:
            p = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=timeout, cwd=HERE)
        except subprocess.TimeoutExpired:
            rec = {'engine': engine, 'error': 'timeout', 'args': args}
        else:
            lines = [ln for ln in p.stdout.splitlines() if ln.startswith('{')]
            if p.returncode != 0 or not lines:
                rec = {'engine': engine, 'error': (p.stderr or p.stdout)[-400:], 'args': args, 'status': p.returncode}
            else:
                rec = json.loads(lines[-1])
        rec.update(section=section, round=rnd, slot=self.slot, wall_s=round(time.monotonic() - t, 3))
        self.slot += 1
        self.emit(rec)
        if 'error' in rec:
            print(f'  {engine} {" ".join(args)}: ERROR {rec["error"][:200]}', file=sys.stderr)
        return rec

    def cli(self, engine: str, file: Path, rnd: int, reps: int) -> None:
        argv, _ = CLIS[engine]
        expected = None
        ns = []
        for i in range(reps + 1):
            t = time.perf_counter_ns()
            p = subprocess.run(['taskset', '-c', CORE, *argv, str(file)], capture_output=True, text=True)
            dt = time.perf_counter_ns() - t
            if p.returncode != 0:
                self.emit({'engine': engine, 'section': 'cli', 'file': file.name, 'round': rnd,
                           'error': p.stderr[-300:]})
                return
            if i == 0:
                expected = p.stdout
            elif p.stdout != expected:
                self.emit({'engine': engine, 'section': 'cli', 'file': file.name, 'round': rnd,
                           'error': 'output changed'})
                return
            else:
                ns.append(dt)
        self.emit({'engine': engine, 'section': 'cli', 'cmd': 'cli', 'file': file.name, 'output': expected,
                   'ns': ns, 'round': rnd, 'slot': self.slot})
        self.slot += 1


def program(name: str, engine: str) -> Path:
    ext = 'py' if engine in PY or CLIS.get(engine, (None, ''))[1] == 'py' else 'js'
    return BENCH / f'{name}.{ext}'


def script(task: str, engine: str) -> Path:
    return SOLUTIONS / f'{task}.py' if engine in PY else HERE / 'js' / f'{task}.js'


def fixture(task: str) -> str:
    return f'fixtures/{task}.json'


def sections(r: Runner, rnd: int, only: set[str]) -> None:
    k = rnd
    want = lambda s: not only or s in only  # noqa: E731

    if want('startup'):
        for e in rotate(list(HOSTS), k):
            r.host(e, ['startup', '20', fixture('neg-fanout-40')], 'startup', rnd)
        k += 1
    if want('mem'):
        for mode in ['empty', 'setup']:
            for n in [1, 8, 40]:
                for e in rotate(list(HOSTS), k):
                    r.host(e, ['mem', mode, str(n), fixture('neg-fanout-40')], 'mem', rnd)
                k += 1
    if want('hostcall'):
        for kind, n, reps in [('sync', 10000, 5), ('await', 10000, 5), ('fanout', 100, 5)]:
            for e in rotate(CODEMODE, k):
                r.host(e, ['hostcall', kind, str(n), str(reps)], 'hostcall', rnd)
            k += 1
    if want('workload'):
        for task in TASKS:
            for e in rotate(CODEMODE, k):
                rec = r.host(e, ['workload', fixture(task), str(script(task, e)), '5'], 'workload', rnd)
                if rnd == 0 and rec and 'check' in rec:
                    with r.calls.open('a') as f:
                        f.write(json.dumps({'engine': e, 'task': task, 'calls': rec['check']['calls'],
                                            'error': rec['check']['error']}) + '\n')
            k += 1
    if want('bench'):
        for name in PROGRAMS:
            for e in rotate(list(HOSTS), k):
                r.host(e, ['bench', str(program(name, e)), '3'], 'bench', rnd)
            for e in rotate(list(CLIS), k):
                r.cli(e, program(name, e), rnd, 3)
            k += 1
    if want('polling'):
        for name in ['loop', 'fib', 'sort']:
            for e in rotate(list(HOSTS), k):
                r.host(e, ['polling', str(program(name, e)), '3'], 'polling', rnd)
            k += 1
    if want('control'):
        for kind in CONTROL:
            for e in rotate(CODEMODE, k):
                r.host(e, ['control', kind], 'control', rnd, timeout=60)
            k += 1
    if want('gc'):
        for items in GC_ITEMS:
            for e in rotate(GC_ENGINES, k):
                r.host(e, ['gc', str(items), '1024'], 'gc', rnd, timeout=300)
                if e == 'mm':
                    r.host(e, ['gc', str(items), '1024', 'requests'], 'gc', rnd, timeout=300)
            k += 1
    if want('snapshot'):
        for task, after in SNAP_POINTS:
            for e in rotate(SNAPSHOTS, k):
                r.host(e, ['snapshot', fixture(task), str(script(task, e)), str(after), '10'], 'snapshot', rnd)
            k += 1


def sizes() -> list[dict]:
    """Bytes and gzip -9 bytes of each engine's minimal one-script host."""
    target = ROOT / 'target'
    files = [
        ('mm', 'native', target / 'x86_64-unknown-linux-gnu/release/examples/hello',
         'micro-monty `hello` (cargo xtask size)'),
        ('rquickjs', 'native', ENG / 'target-size-rquickjs/x86_64-unknown-linux-gnu/release/size-rquickjs',
         'size/rquickjs (build.sh)'),
        ('djs', 'native', ENG / 'target-size-djs/x86_64-unknown-linux-gnu/release/size-djs',
         'size/djs (build.sh)'),
        ('qjs-ng', 'native', ENG / 'bin/qjs-ng', 'quickjs-ng qjs CLI (cmake Release, IPO, stripped)'),
        ('qjs-bellard', 'native', ENG / 'bin/qjs-bellard', 'QuickJS qjs CLI (make CONFIG_LTO=y, stripped)'),
        ('djs-cli', 'native', ENG / 'bin/djs', 'djs CLI (upstream release profile, stripped)'),
        ('mm', 'wasm32-unknown-unknown', target / 'wasm32-unknown-unknown/release/shipped/hello.wasm',
         'micro-monty `hello.wasm` (cargo xtask size)'),
        ('mm-wasm guest', 'wasm32-unknown-unknown', target / 'wasm32-unknown-unknown/shipped/mm_wasm_guest.wasm',
         'mm-wasm v1 guest (cargo xtask size)'),
        ('quickjs-wasi', 'wasm32-wasip1', ENG / 'qwasi/quickjs.wasm', 'quickjs-wasi 3.6.2 npm module'),
        ('djs-wasm', 'wasm32-unknown-unknown', ENG / 'wasm/djs_wasm.wasm',
         'djs scripts/build-wasm.sh, then wasm-opt -Oz'),
    ]
    out = []
    for engine, target_name, path, how in files:
        if not path.exists():
            out.append({'engine': engine, 'target': target_name, 'missing': str(path)})
            continue
        data = path.read_bytes()
        out.append({'engine': engine, 'target': target_name, 'bytes': len(data),
                    'gzip9': len(gzip.compress(data, 9)), 'how': how, 'path': str(path)})
    return out


def med(xs: list[float]) -> float | None:
    return statistics.median(xs) if xs else None


def summarize(out: Path) -> dict:
    recs = [json.loads(ln) for ln in (out / 'raw.jsonl').read_text().splitlines()]
    s: dict = {}

    def put(path: list[str], value) -> None:
        d = s
        for p in path[:-1]:
            d = d.setdefault(p, {})
        d[path[-1]] = value

    def gather(sel, field):
        xs = []
        for r in recs:
            if sel(r) and isinstance(r.get(field), list):
                xs.extend(r[field])
            elif sel(r) and isinstance(r.get(field), (int, float)):
                xs.append(r[field])
        return xs

    engines = sorted({r['engine'] for r in recs if 'engine' in r})
    errors = [r for r in recs if 'error' in r]
    put(['errors'], [{k: r.get(k) for k in ('engine', 'section', 'args', 'error', 'file')} for r in errors])
    for e in engines:
        is_e = lambda r, sec, e=e: r.get('engine') == e and r.get('section') == sec and 'error' not in r  # noqa: E731
        for f in ['runtime_ns', 'session_ns', 'setup_ns', 'first_cell_ns', 'image_ns', 'image_first_cell_ns']:
            v = med(gather(lambda r: is_e(r, 'startup'), f))
            if v is not None:
                put(['startup', e, f], v)
        for mode in ['empty', 'setup']:
            for n in [1, 8, 40]:
                sel = [r for r in recs if is_e(r, 'mem') and r['mode'] == mode and r['n'] == n]
                if sel:
                    put(['mem', e, f'{mode}-{n}'], {
                        'rss_delta': med([r['rss_after'] - r['rss_before'] for r in sel]),
                        'heap_delta': med([r['heap_after'] - r['heap_before'] for r in sel]),
                        'rss_after': med([r['rss_after'] for r in sel]),
                        'engine_bytes': med([r['engine_bytes'] for r in sel]),
                    })
        for kind in ['sync', 'await', 'fanout']:
            sel = [r for r in recs if is_e(r, 'hostcall') and r['kind'] == kind]
            if sel:
                calls = sel[0]['n'] * (100 if kind == 'fanout' else 1)
                tool = med([x for r in sel for x in r['tool_ns']])
                local = med([x for r in sel for x in r['local_ns']])
                put(['hostcall', e, kind], {'tool_ns': tool, 'local_ns': local, 'calls': calls,
                                            'per_call_ns': (tool - local) / calls,
                                            'per_call_total_ns': tool / calls})
        for task in TASKS:
            sel = [r for r in recs if is_e(r, 'workload') and r['task'] == task]
            if sel:
                put(['workload', e, task], {f: med([x for r in sel for x in r[f]])
                                            for f in ['ns', 'setup_ns', 'run_ns']})
        for sec in ['bench', 'cli']:
            for name in PROGRAMS:
                sel = [r for r in recs if r.get('engine') == e and r.get('section') == sec and 'error' not in r
                       and Path(r.get('file', '')).stem == name]
                if sel:
                    put([sec, e, name], {'ns': med([x for r in sel for x in r['ns']]),
                                         'output': sel[0]['output'].strip()})
        for name in ['loop', 'fib', 'sort']:
            sel = [r for r in recs if is_e(r, 'polling') and Path(r['file']).stem == name]
            if sel:
                p, q = med([x for r in sel for x in r['plain_ns']]), med([x for r in sel for x in r['polled_ns']])
                put(['polling', e, name], {'plain_ns': p, 'polled_ns': q, 'overhead': q / p - 1})
        for kind in CONTROL:
            sel = [r for r in recs if r.get('engine') == e and r.get('section') == 'control'
                   and r.get('kind') == kind and 'error' not in r]
            if sel:
                lat = [r['latency_ns'] for r in sel if isinstance(r.get('latency_ns'), (int, float))]
                put(['control', e, kind], {'how': sorted({r['how'] for r in sel}),
                                           'resumable': sorted({r.get('resumable') for r in sel}, key=str),
                                           'session_usable': sorted({r.get('session_usable') for r in sel}, key=str),
                                           'latency_ns': med(lat), 'n': len(sel)})
        for items, mode in [(i, m) for i in GC_ITEMS for m in ['slices', 'requests']]:
            sel = [r for r in recs if is_e(r, 'gc') and r.get('supported') and r['items'] == items
                   and r.get('mode', 'slices') == mode]
            if sel:
                d = {'full_ns': med([x for r in sel for x in r['full_ns']]),
                     'live_heap': med([r['live_heap'] for r in sel]),
                     'engine_bytes': med([r['engine_bytes'] or 0 for r in sel]), 'n': len(sel)}
                for kind in sorted({w['kind'] for r in sel for w in r['workloads']}):
                    ws = [w for r in sel for w in r['workloads'] if w['kind'] == kind]
                    errs = sorted({w['error'] for w in ws if w.get('error')})
                    ws = [w for w in ws if not w.get('error')]
                    if not ws:
                        d[kind] = {'error': errs}
                        continue
                    d[kind] = {'errors': errs, 'max_ns': med([w['max_ns'] for w in ws]), 'worst_ns': max(w['max_ns'] for w in ws),
                               'p99_ns': med([w['p99_ns'] for w in ws]), 'p50_ns': med([w['p50_ns'] for w in ws]),
                               'wall_ns': med([w['wall_ns'] for w in ws])}
                put(['gc', e if mode == 'slices' else f'{e} (park requests)', str(items)], d)
        for task, after in SNAP_POINTS:
            sel = [r for r in recs if is_e(r, 'snapshot') and r.get('task') == task]
            if sel:
                put(['snapshot', e, f'{task}@{after}'], {
                    'kind': sel[0]['kind'], 'raw_bytes': sel[0]['raw_bytes'], 'zstd1_bytes': sel[0]['zstd1_bytes'],
                    'snapshot_ns': med([x for r in sel for x in r['snapshot_ns']]),
                    'restore_ns': med([x for r in sel for x in r['restore_ns']]),
                    'finished': sorted({json.dumps(r['finished']) for r in sel})})
            elif any(r.get('engine') == e and r.get('section') == 'snapshot' and r.get('supported') is False
                     for r in recs):
                put(['snapshot', e, f'{task}@{after}'], {'supported': False})
    return s


ORDER = ['mm-native', 'mm-wasm', 'rquickjs', 'dquickts', 'quickjs-wasi', 'djs', 'djs-wasm',
         'qjs-ng', 'qjs-bellard', 'djs-cli', 'mm-cli']


def markdown(s: dict, sizes_: list[dict]) -> str:
    """The summary's tables, medians over every round."""
    def ms(ns, d=2):
        return '—' if ns is None else f'{ns / 1e6:.{d}f}'

    def us(ns, d=1):
        return '—' if ns is None else f'{ns / 1e3:.{d}f}'

    def eng(section):
        return [e for e in ORDER if e in s.get(section, {})]

    out = ['# Engine comparison: medians', ''] + headline(s, sizes_)
    out += ['## Size (bytes; gzip -9)', '', '| Engine | Target | Bytes | gzip -9 | Build |', '|---|---|---:|---:|---|']
    for x in sizes_:
        if 'bytes' in x:
            out.append(f'| {x["engine"]} | {x["target"]} | {x["bytes"]:,} | {x["gzip9"]:,} | {x["how"]} |')
    out += ['', '## Start-up (µs)', '',
            '| Engine | New runtime | New session | Codemode setup | First cell | From image | First cell after image |',
            '|---|---:|---:|---:|---:|---:|---:|']
    for e in eng('startup'):
        d = s['startup'][e]
        out.append(f'| {e} | ' + ' | '.join(us(d.get(f)) for f in [
            'runtime_ns', 'session_ns', 'setup_ns', 'first_cell_ns', 'image_ns', 'image_first_cell_ns']) + ' |')
    out += ['', '## Memory (KB added by N live sessions: RSS / malloc heap / engine count)', '',
            '| Engine | ' + ' | '.join(f'{m} {n}' for m in ['empty', 'setup'] for n in [1, 8, 40]) + ' |',
            '|---|' + '---:|' * 6]
    for e in eng('mem'):
        d = s['mem'][e]
        cells = []
        for m in ['empty', 'setup']:
            for n in [1, 8, 40]:
                v = d.get(f'{m}-{n}')
                cells.append('—' if v is None else
                             f'{v["rss_delta"] / 1e3:,.0f} / {v["heap_delta"] / 1e3:,.0f} / {v["engine_bytes"] / 1e3:,.0f}')
        out.append(f'| {e} | ' + ' | '.join(cells) + ' |')
    out += ['', '## Host calls (µs per call: tool minus local, and total)', '',
            '| Engine | Sync | Sync total | Awaited | Awaited total | Fan-out of 100 | Fan-out total |',
            '|---|---:|---:|---:|---:|---:|---:|']
    for e in eng('hostcall'):
        d = s['hostcall'][e]
        cells = []
        for k in ['sync', 'await', 'fanout']:
            v = d.get(k)
            cells += ['—', '—'] if v is None else [us(v['per_call_ns'], 2), us(v['per_call_total_ns'], 2)]
        out.append(f'| {e} | ' + ' | '.join(cells) + ' |')
    out += ['', '## Codemode workload (ms per task: new session + setup + run)', '']
    engines = eng('workload')
    out += ['| Task | ' + ' | '.join(engines) + ' |', '|---|' + '---:|' * len(engines)]
    tasks = sorted({t for e in engines for t in s['workload'][e]})
    for t in tasks:
        out.append(f'| {t} | ' + ' | '.join(
            '—' if t not in s['workload'][e] else
            f'{ms(s["workload"][e][t]["ns"])} ({ms(s["workload"][e][t]["run_ns"])})' for e in engines) + ' |')
    for f, label in [('ns', 'sum, total'), ('setup_ns', 'sum, setup'), ('run_ns', 'sum, run')]:
        out.append(f'| **{label}** | ' + ' | '.join(
            ms(sum(v[f] for v in s['workload'][e].values()), 1) for e in engines) + ' |')
    out += ['', '(run only in parentheses)', '']
    for sec, title in [('bench', 'Bench programs in-process (ms per run)'),
                       ('cli', 'Bench programs as CLI processes (ms, spawn to exit)')]:
        engines = eng(sec)
        out += [f'## {title}', '', '| Program | ' + ' | '.join(engines) + ' |', '|---|' + '---:|' * len(engines)]
        for p in PROGRAMS:
            out.append(f'| {p} | ' + ' | '.join(
                ms(s[sec][e][p]['ns'], 1) if p in s[sec][e] else '—' for e in engines) + ' |')
        out.append('')
    out += ['## Interrupt polling / fuel slices (overhead on the plain run)', '',
            '| Engine | loop | fib | sort |', '|---|---:|---:|---:|']
    for e in eng('polling'):
        d = s['polling'][e]
        out.append(f'| {e} | ' + ' | '.join(
            f'{d[p]["overhead"] * 100:+.1f}%' if p in d else '—' for p in ['loop', 'fib', 'sort']) + ' |')
    out += ['', '## Control (ms to regain control after the stop request; outcome)', '',
            '| Engine | ' + ' | '.join(CONTROL) + ' |', '|---|' + '---|' * len(CONTROL)]
    for e in eng('control'):
        d = s['control'][e]
        cells = []
        for k in CONTROL:
            v = d.get(k)
            if v is None:
                cells.append('—')
                continue
            how = '; '.join(v['how'])
            lat = '' if v['latency_ns'] is None else f'{ms(v["latency_ns"])} ms, '
            flags = ('resumable' if v['resumable'] == [True] else 'not resumable') + \
                (', instance kept' if v['session_usable'] == [True] else
                 ', instance lost' if v['session_usable'] == [False] else '')
            cells.append(f'{lat}{how} ({flags}, n={v["n"]})' if 'not regained' not in how
                         else f'not regained in 10 s (n={v["n"]})')
        out.append(f'| {e} | ' + ' | '.join(cells) + ' |')
    out += ['', '## Snapshots mid-await', '',
            '| Engine | Point | Kind | Raw bytes | zstd-1 bytes | Snapshot µs | Restore µs | Finished |',
            '|---|---|---|---:|---:|---:|---:|---|']
    for e in eng('snapshot'):
        for point, v in s['snapshot'][e].items():
            if v.get('supported') is False:
                out.append(f'| {e} | {point} | unsupported | | | | | |')
                continue
            fin = all(json.loads(f)['ok'] and not json.loads(f)['misses'] for f in v['finished'])
            out.append(f'| {e} | {point} | {v["kind"]} | {v["raw_bytes"]:,} | {v["zstd1_bytes"]:,} | '
                       f'{us(v["snapshot_ns"])} | {us(v["restore_ns"])} | {"ok" if fin else "FAILED"} |')
    out += ['', '## GC (live pairs, 1 GiB limit)', '',
            'Full: one explicit full collection, ms. Workloads: the longest gap between the',
            "engine's control points (10k-fuel slices, QuickJS's interrupt polls), median of the",
            "rounds' maxima (worst round in brackets), ms; for mm (park requests), the longest",
            'latency from a park request (every 1 ms) to the park. Then the median gap in µs.', '',
            '| Engine | Items | Live heap MB | Full | garbage | cycles | writes | appends | median gap µs |',
            '|---|---:|---:|---:|---:|---:|---:|---:|---:|']
    for e in [x for o in ORDER for x in (o, f'{o} (park requests)') if x in s.get('gc', {})]:
        for items, d in sorted(s['gc'][e].items(), key=lambda x: int(x[0])):
            ws = [d.get(k) for k in ['garbage', 'cycles', 'writes', 'appends']]
            out.append(f'| {e} | {int(items):,} | {d["live_heap"] / 1e6:,.0f} | {ms(d["full_ns"], 1)} | ' + ' | '.join(
                '—' if w is None else 'error' if 'error' in w else
                f'{ms(w["max_ns"], 1)} ({ms(w["worst_ns"], 1)})' for w in ws) +
                f' | {us(d["garbage"]["p50_ns"], 0)} |')
    if s.get('errors'):
        out += ['', '## Errors', ''] + [f'- {x}' for x in s['errors']]
    return '\n'.join(out) + '\n'


def geomean(xs: list[float]) -> float:
    return math.exp(sum(math.log(x) for x in xs) / len(xs))  # ZeroDivisionError when empty: "—"


def headline(s: dict, sizes_: list[dict]) -> list[str]:
    """The doc's summary figures, derived from the medians."""
    out = ['## Headline', '', '| Figure | ' + ' | '.join(ORDER[:7]) + ' |', '|---|' + '---:|' * 7]

    def row(label, f):
        cells = []
        for e in ORDER[:7]:
            try:
                v = f(e)
            except (KeyError, TypeError, ZeroDivisionError, StopIteration):
                v = None
            cells.append('—' if v is None else v)
        out.append(f'| {label} | ' + ' | '.join(cells) + ' |')

    size_of = {'mm-native': ('mm', 'native'), 'mm-wasm': ('mm', 'wasm32-unknown-unknown'),
               'rquickjs': ('rquickjs', 'native'), 'djs': ('djs', 'native'),
               'quickjs-wasi': ('quickjs-wasi', 'wasm32-wasip1'), 'djs-wasm': ('djs-wasm', 'wasm32-unknown-unknown')}
    row('Minimal one-script host, bytes', lambda e: next(
        f'{x["bytes"]:,}' for x in sizes_ if (x['engine'], x['target']) == size_of[e] and 'bytes' in x))
    row('RSS per set-up session (40 live), KB', lambda e: f'{s["mem"][e]["setup-40"]["rss_delta"] / 40 / 1e3:,.0f}')
    row('RSS per empty session (40 live), KB', lambda e: f'{s["mem"][e]["empty-40"]["rss_delta"] / 40 / 1e3:,.0f}')
    row('New session + codemode setup, ms', lambda e: f'{(s["startup"][e]["session_ns"] + s["startup"][e]["setup_ns"]) / 1e6:.2f}')
    row('Session from a saved image, ms', lambda e: f'{s["startup"][e]["image_ns"] / 1e6:.3f}')
    for k, label in [('await', 'Awaited host call'), ('fanout', 'Fan-out of 100'), ('sync', 'Sync host call')]:
        row(f'{label}, µs per call', lambda e, k=k: f'{s["hostcall"][e][k]["per_call_ns"] / 1e3:.2f}')
    for f, label in [('ns', '22 tasks end to end (sum), ms'), ('setup_ns', '…setup only (sum), ms'),
                     ('run_ns', '…script run only (sum), ms')]:
        row(label, lambda e, f=f: f'{sum(v[f] for v in s["workload"][e].values()) / 1e6:.1f}')
    row('Task end to end, median, ms', lambda e: f'{statistics.median(v["ns"] for v in s["workload"][e].values()) / 1e6:.2f}')
    mm_w = s.get('workload', {}).get('mm-native', {})
    row('Script run vs mm (geomean)', lambda e: f'{geomean([s["workload"][e][t]["run_ns"] / mm_w[t]["run_ns"] for t in mm_w]):.2f}')
    progs = [p for p in PROGRAMS if p != 'startup']
    mm_b = s.get('bench', {}).get('mm-native', {})
    row('12 bench programs, time vs mm (geomean)',
        lambda e: f'{geomean([s["bench"][e][p]["ns"] / mm_b[p]["ns"] for p in progs]):.2f}')
    out.append('')
    mm_c = s.get('cli', {}).get('mm-cli', {})
    out.append('CLI bench geomean vs `mm run`: ' + ', '.join(
        f'{e} {geomean([s["cli"][e][p]["ns"] / mm_c[p]["ns"] for p in progs]):.2f}'
        for e in ORDER[7:] if e in s.get('cli', {}) and mm_c) + '.')
    out.append('')
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--rounds', type=int, default=5)
    ap.add_argument('--only', default='')
    ap.add_argument('--out')
    ap.add_argument('--summarize-only', action='store_true')
    ap.add_argument('--no-setup', action='store_true')
    a = ap.parse_args()
    head = subprocess.run(['git', 'rev-parse', '--short=7', 'HEAD'], capture_output=True, text=True,
                          cwd=ROOT).stdout.strip()
    out = Path(a.out or HERE / 'results' / f'{time.strftime("%Y-%m-%d")}-{head}')
    out.mkdir(parents=True, exist_ok=True)
    only = {x for x in a.only.split(',') if x}
    if not a.summarize_only:
        if not a.no_setup:
            subprocess.run([str(HERE / 'setup.sh')], check=True)
        dirty = subprocess.run(['git', 'status', '--porcelain', '--', 'crates', 'Cargo.lock'],
                               capture_output=True, text=True, cwd=ROOT).stdout.strip()
        (out / 'meta.json').write_text(json.dumps({
            'commit': subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True,
                                     cwd=ROOT).stdout.strip(),
            'engine_crates_dirty': bool(dirty), 'rounds': a.rounds, 'only': sorted(only), 'core': CORE,
            'started': time.strftime('%Y-%m-%dT%H:%M:%S%z'),
            'cpu': next((ln.split(':', 1)[1].strip() for ln in Path('/proc/cpuinfo').read_text().splitlines()
                         if ln.startswith('model name')), None),
            'nproc': os.cpu_count(),
        }, indent=1) + '\n')
        r = Runner(out)
        for e, path in HOSTS.items():
            rec = r.host(e, ['describe'], 'describe', -1)
            if rec and 'describe' in rec:
                rec['sha_prefix'] = subprocess.run(['sha256sum', str(path)], capture_output=True,
                                                   text=True).stdout[:16]
        (out / 'sizes.json').write_text(json.dumps(sizes(), indent=1) + '\n')
        for rnd in range(a.rounds):
            print(f'round {rnd + 1}/{a.rounds}', file=sys.stderr, flush=True)
            sections(r, rnd, only)
        if r.calls.exists():
            p = subprocess.run([PYTHON, str(HERE / 'check.py'), str(r.calls)], capture_output=True, text=True)
            (out / 'check.txt').write_text(p.stdout + p.stderr)
            print(p.stdout.count('pass'), 'workload checks pass;', p.stdout.count('FAIL'), 'fail', file=sys.stderr)
    summary = summarize(out)
    (out / 'summary.json').write_text(json.dumps(summary, indent=1, sort_keys=True) + '\n')
    sizes_ = json.loads((out / 'sizes.json').read_text()) if (out / 'sizes.json').exists() else []
    (out / 'summary.md').write_text(markdown(summary, sizes_))
    return 0


if __name__ == '__main__':
    sys.exit(main())
