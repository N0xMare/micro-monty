"""Runs the Workers / V8 measurements (docs/research/workers-v8.md).

    python3.14 -I hosts/js/bench/run.py [--rounds N] [--only node,k,workerd,hib] [--out DIR]

Needs the guests (`npm run guests`), `npm install` in hosts/js, and pi's
codemode source at the harness's pin ($PI_CODEMODE, default
/tmp/eng/pi/packages/codemode, with node_modules/quickjs-wasi linked to
hosts/js/node_modules/quickjs-wasi; see hosts/js/README.md).

Every measurement is a fresh process pinned to cores 2-5 (`taskset -c $CORE`;
V8 needs its background threads), the engine order rotating each round, as bench/evidence/engines/run.py
does: `node --expose-gc bench/node.ts ...` for Node, and one `workerd serve` per
engine and round for workerd (the Worker bundle of bench/worker, the engine's wasm
as its `engine.wasm` module). Each record goes to DIR/raw.jsonl; DIR/summary.json
holds medians over rounds; workload calls are judged by
bench/evidence/engines/check.py (DIR/check.txt). The Durable Object demo parks a
cell in SQLite storage, kills workerd, starts a new one and resumes it there.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import statistics
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
JS = HERE.parent
ROOT = JS.parents[1]
# Four cores: V8 compiles lazily, tiers up and collects garbage on background
# threads, and a single pinned core made warm sessions 4x slower and noisy [M].
CORE = os.environ.get('CORE', '2-5')
PYTHON = os.environ.get('PYTHON') or str(Path.home() / '.local/bin/python3.14')
NODE = os.environ.get('NODE', 'node')
WORKERD = str(JS / 'node_modules/.bin/workerd')
GUESTS = Path(os.environ.get('MM_GUEST_DIR', JS / 'guests'))
WASM = {
    'mm:default': GUESTS / 'default.wasm',
    'mm:core35': GUESTS / 'core35.wasm',
    'mm:nostdlib': GUESTS / 'nostdlib.wasm',
    'pi': JS / 'node_modules/quickjs-wasi/quickjs.wasm',
}
ALL = list(WASM)
MAIN = ['mm:default', 'pi']
PORT = 8811
# len(FUEL_PROBES) in bench/measure.ts.
N_PROBES = 26


def rotate(xs, k):
    k %= len(xs)
    return xs[k:] + xs[:k]


class Out:
    def __init__(self, out: Path):
        self.out = out
        out.mkdir(parents=True, exist_ok=True)
        self.raw = out / 'raw.jsonl'

    def emit(self, rec):
        with self.raw.open('a') as f:
            f.write(json.dumps(rec) + '\n')
        if 'error' in rec:
            print(f'  ERROR {rec.get("section")} {rec.get("engine")}: {str(rec["error"])[:300]}', file=sys.stderr)


def node(o: Out, section, rnd, engine, args, flags=()):
    cmd = ['taskset', '-c', CORE, NODE, '--expose-gc', *flags, 'bench/node.ts', args[0], engine, *args[1:]]
    p = subprocess.run(cmd, cwd=JS, capture_output=True, text=True, timeout=900)
    lines = [ln for ln in p.stdout.splitlines() if ln.startswith('{')]
    rec = json.loads(lines[-1]) if p.returncode == 0 and lines else {'engine': engine, 'error': p.stderr[-800:]}
    rec.update(section=section, round=rnd, flags=list(flags))
    o.emit(rec)
    return rec


# ------------------------------------------------------------------ workerd

CONFIG = '''using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
  services = [
    (name = "main", worker = .w),
    (name = "do-disk", disk = (path = "{disk}", writable = true)),
  ],
  sockets = [ (name = "http", address = "127.0.0.1:{port}", http = (), service = "main") ],
);
const w :Workerd.Worker = (
  modules = [
    (name = "worker.js", esModule = embed "worker.js"),
    (name = "engine.wasm", wasm = embed "engine.wasm"),
  ],
  compatibilityDate = "2026-11-01",
  bindings = [
    (name = "ENGINE", text = "{engine}"),
    (name = "CELLS", durableObjectNamespace = "Cells"),
  ],
  durableObjectNamespaces = [ (className = "Cells", uniqueKey = "mm-workers-v8", enableSql = true) ],
  durableObjectStorage = (localDisk = "do-disk"),
);
'''
# The smallest valid module: a worker without an engine (startup baseline).
EMPTY_WASM = b'\0asm\1\0\0\0'


def prepare(engine: str, base: Path) -> Path:
    """`ENGINE+warm` uses the bundle that runs one session in the global scope."""
    name, warm = engine.removesuffix('+warm'), engine.endswith('+warm')
    d = base / engine.replace(':', '-').replace('+', '-')
    (d / 'do').mkdir(parents=True, exist_ok=True)
    shutil.copy(JS / ('.bench/worker-warm.js' if warm else '.bench/worker.js'), d / 'worker.js')
    if name == 'none':
        (d / 'engine.wasm').write_bytes(EMPTY_WASM)
    else:
        shutil.copy(WASM[name], d / 'engine.wasm')
    (d / 'config.capnp').write_text(CONFIG.format(disk=d / 'do', port=PORT, engine=name))
    return d


class Workerd:
    """One `workerd serve` process; `ready_ms` is spawn to the first answered request."""

    def __init__(self, d: Path):
        t = time.perf_counter()
        self.p = subprocess.Popen(['taskset', '-c', CORE, WORKERD, 'serve', 'config.capnp'], cwd=d,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        while True:
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{PORT}/ping', timeout=5).read()
                break
            except OSError:
                if self.p.poll() is not None:
                    raise RuntimeError('workerd exited')
                time.sleep(0.002)
        self.ready_ms = (time.perf_counter() - t) * 1000

    def get(self, path: str, timeout=600):
        t = time.perf_counter()
        with urllib.request.urlopen(f'http://127.0.0.1:{PORT}{path}', timeout=timeout) as r:
            body = json.loads(r.read())
        body['request_ms'] = (time.perf_counter() - t) * 1000
        return body

    def rss(self) -> int:
        for line in Path(f'/proc/{self.p.pid}/status').read_text().splitlines():
            if line.startswith('VmRSS:'):
                return int(line.split()[1]) * 1024
        return 0

    def stop(self):
        self.p.send_signal(signal.SIGKILL)
        self.p.wait()


def workerd_round(o: Out, rnd: int, base: Path):
    for engine in rotate(['none', *ALL, 'mm:default+warm', 'mm:core35+warm', 'pi+warm'], rnd):
        d = prepare(engine, base)
        w = Workerd(d)
        rec = {'section': 'workerd', 'round': rnd, 'engine': engine, 'ready_ms': w.ready_ms,
               'rss_ready': w.rss()}
        try:
            if engine != 'none':
                rec['startup'] = w.get('/startup?reps=20')
            if engine != 'none' and not engine.endswith('+warm'):
                w.get('/mem/add?n=1')
                rss0 = w.rss()
                time.sleep(0.2)
                m = w.get('/mem/add?n=40')
                time.sleep(0.2)
                rec['mem'] = {'n': 40, 'rss_before': rss0, 'rss_after': w.rss(), 'linear_after': m['linear']}
                w.get('/mem/clear')
                if engine in MAIN:
                    rec['hostcall_await'] = w.get('/hostcall?kind=await&n=2000&reps=5')
                    rec['hostcall_fanout'] = w.get('/hostcall?kind=fanout&n=20&reps=5')
                    rec['snapshot'] = w.get('/snapshot')
                rec['workload'] = w.get('/workload?reps=5')
                rec['rss_end'] = w.rss()
        except Exception as e:  # noqa: BLE001
            rec['error'] = repr(e)
        finally:
            w.stop()
        o.emit(rec)


K_SESSIONS = [(k, setup) for k in (1, 4, 16) for setup in (0, 1)]


def node_k_fuel(o: Out, rnd: int):
    """K sessions per instance and fuel rates, mm's default build in Node."""
    e = 'mm:default'
    for k, setup in rotate(K_SESSIONS, rnd):
        node(o, 'ksessions', rnd, e, ['ksessions', str(k), '32', 'setup' if setup else 'bare'])
    node(o, 'fuel', rnd, e, ['fuel', 'programs', '3'])
    node(o, 'fuel', rnd, e, ['fuel', 'tasks', '3'])
    node(o, 'fuel', rnd, e, ['fuel', 'worst', '16'])
    if rnd == 0:
        node(o, 'fuel', rnd, e, ['fuel', 'worst', '256'])


def workerd_k_fuel(o: Out, rnd: int, base: Path):
    """The same in workerd: one process per round for K sessions, one for fuel."""
    e = 'mm:default'
    w = Workerd(prepare(e, base))
    rec = {'section': 'workerd-k', 'round': rnd, 'engine': e, 'k': []}
    try:
        for k, setup in rotate(K_SESSIONS, rnd):
            rec['k'].append(w.get(f'/ksessions?k={k}&n=32&setup={setup}'))
    except Exception as ex:  # noqa: BLE001
        rec['error'] = repr(ex)
    finally:
        w.stop()
    o.emit(rec)
    w = Workerd(prepare(e, base))
    rec = {'section': 'workerd-fuel', 'round': rnd, 'engine': e}
    try:
        rec['programs'] = w.get('/fuel/programs?reps=3')['programs']
        rec['tasks'] = w.get('/fuel/tasks?reps=3')
        for mib in (16, 256) if rnd == 0 else (16,):
            rec[f'worst_{mib}'] = [w.get(f'/fuel/worst?mib={mib}&probe={i}') for i in range(N_PROBES)]
    except Exception as ex:  # noqa: BLE001
        rec['error'] = repr(ex)
    finally:
        w.stop()
    o.emit(rec)


def hib(o: Out, base: Path, rnd: int):
    """Park a cell in a Durable Object, kill workerd, resume it in a new process."""
    for task, after in [('neg-fanout-40', 0), ('agg-avg-order-by-tier', 3), ('par-two-level-fanout', 1)]:
        d = prepare('mm:default', base)
        shutil.rmtree(d / 'do')
        (d / 'do').mkdir()
        w = Workerd(d)
        start = w.get(f'/hib/start?task={task}&after={after}')
        w.stop()
        w2 = Workerd(d)
        resume = w2.get('/hib/resume')
        w2.stop()
        o.emit({'section': 'hib', 'round': rnd, 'engine': 'mm:default', 'task': task, 'after': after,
                'start': start, 'resume': resume, 'ready_ms': w2.ready_ms})


# ------------------------------------------------------------------ summary

def med(xs):
    xs = [x for x in xs if isinstance(x, (int, float))]
    return statistics.median(xs) if xs else None


def summarize(out: Path):
    recs = [json.loads(ln) for ln in (out / 'raw.jsonl').read_text().splitlines()]
    ok = [r for r in recs if 'error' not in r]
    s: dict = {}

    def by(section, engine, flags=None):
        return [r for r in ok if r['section'] == section and r['engine'] == engine
                and (flags is None or r.get('flags') == flags)]

    for e in ALL:
        st = by('startup', e, [])
        eager = by('startup', e, ['--no-wasm-lazy-compilation'])
        mem = by('mem', e)
        wl = by('workload', e)
        s[f'node {e}'] = {
            'compile_ms': med([r['compile_ms'] for r in st]),
            'compile_eager_ms': med([r['compile_ms'] for r in eager]),
            'first_instance_ms': med([r['runtime_ms'] for r in st]),
            'cold_total_ms': med([r['cold']['total'] for r in st]),
            'cold_total_eager_ms': med([r['cold']['total'] for r in eager]),
            'cold_session_ms': med([r['cold']['session'] for r in st]),
            'cold_setup_ms': med([r['cold']['setup'] for r in st]),
            'cold_first_cell_ms': med([r['cold']['first'] for r in st]),
            'warm_total_ms': med([r['warm_total_ms'] for r in st]),
            'warm_session_ms': med([r['warm_session_ms'] for r in st]),
            'warm_setup_ms': med([r['warm_setup_ms'] for r in st]),
            'warm_first_cell_ms': med([r['warm_first_cell_ms'] for r in st]),
            'mem_rss_per_session': med([r['per_session']['rss'] for r in mem]),
            'mem_linear_per_session': med([r['per_session']['linear'] for r in mem]),
            'mem_heap_per_session': med([r['per_session']['heap'] for r in mem]),
            'mem_rss_base': med([r['before']['rss'] for r in mem]),
            'workload_sum_ms': med([r['sum_total_ms'] for r in wl]),
            'workload_setup_sum_ms': med([sum(t['setup_ms'] for t in r['tasks']) for r in wl]) if e != 'pi' else None,
            'workload_failed': sorted({t['task'] for r in wl for t in r['tasks'] if t['check']['error']}),
            'workload_tasks': {t: med([x['total_ms'] for r in wl for x in r['tasks'] if x['task'] == t])
                               for t in sorted({x['task'] for r in wl for x in r['tasks']})},
        }
        for kind in ['await', 'fanout']:
            hc = [r for r in by('hostcall', e) if r['kind'] == kind]
            s[f'node {e}'][f'hostcall_{kind}_us'] = med([r['per_call_us'] for r in hc])
        cap = [r for r in recs if r['section'] == 'memcap' and r['engine'] == e]
        if cap:
            s[f'node {e}']['memcap'] = {str(r['mib']): r for r in cap}
        snap = by('snapshot', e)
        if snap:
            s[f'node {e}']['snapshot'] = [
                {k: (med([r['points'][i][k] for r in snap]) if isinstance(p[k], (int, float)) else p[k])
                 for k in p} for i, p in enumerate(snap[0]['points'])]
    for e in ['none', *ALL, 'mm:default+warm', 'mm:core35+warm', 'pi+warm']:
        rs = by('workerd', e)
        if not rs:
            continue
        x = {'ready_ms': med([r['ready_ms'] for r in rs]), 'rss_ready': med([r['rss_ready'] for r in rs])}
        if e != 'none':
            x.update({
                'cold_total_ms': med([r['startup']['cold']['total'] for r in rs]),
                'cold_setup_ms': med([r['startup']['cold']['setup'] for r in rs]),
                'warm_total_mean_ms': med([r['startup']['warm_total_mean_ms'] for r in rs]),
            })
        if e != 'none' and not e.endswith('+warm'):
            x.update({
                'mem_rss_per_session': med([(r['mem']['rss_after'] - r['mem']['rss_before']) / 40 for r in rs]),
                'mem_linear_41': med([r['mem']['linear_after'] for r in rs]),
                'rss_after_41': med([r['mem']['rss_after'] for r in rs]),
                'workload_sum_ms': med([r['workload']['sum_total_ms'] for r in rs]),
                'workload_failed': sorted({t['task'] for r in rs for t in r['workload']['tasks']
                                           if t['check']['error']}),
            })
            if 'hostcall_await' in rs[0]:
                x['hostcall_await_us'] = med([r['hostcall_await']['per_call_us'] for r in rs])
                x['hostcall_fanout_us'] = med([r['hostcall_fanout']['per_call_us'] for r in rs])
            if 'snapshot' in rs[0] and 'points' in rs[0]['snapshot']:
                x['snapshot'] = [{k: med([r['snapshot']['points'][i][k] for r in rs]) for k in
                                  ['raw', 'gzip', 'snapshot_ms', 'restore_ms', 'linear']} | {'point': p['point']}
                                 for i, p in enumerate(rs[0]['snapshot']['points'])]
        s[f'workerd {e}'] = x
    # K sessions per instance and fuel rates (mm:default).
    def ksum(runs):
        out = {}
        for k, setup in K_SESSIONS:
            rs = [r for r in runs if r['k'] == k and bool(r['setup']) == bool(setup)]
            if rs:
                out[f'k{k} {"setup" if setup else "bare"}'] = {
                    f: med([r.get(f) for r in rs]) for f in
                    ['instances', 'linear_per_session', 'rss_per_session', 'create_mean_ms',
                     'new_instance_mean_ms', 'shared_slot_mean_ms']}
        return out

    def fsum(prog_runs, task_runs, worst):
        x = {}
        if prog_runs:
            names = [p['name'] for p in prog_runs[0]]
            x['programs'] = {n: {f: med([p[f] for ps in prog_runs for p in ps if p['name'] == n]) for f in
                                 ['ms', 'fuel', 'us_per_fuel', 'granted', 'us_per_granted', 'max_slice_ms', 'max_overdraft']}
                             | {'ok': all(p['ok'] for ps in prog_runs for p in ps if p['name'] == n)}
                             for n in names}
        if task_runs:
            x['tasks'] = {f: med([t[f] for t in task_runs]) for f in ['sum_ms', 'sum_fuel', 'us_per_fuel']}
            x['tasks']['max_fuel'] = max(t['fuel'] for r in task_runs for t in r['tasks'])
            x['tasks']['errors'] = sorted({t['task'] for r in task_runs for t in r['tasks'] if t['error']})
            x['tasks']['per_task'] = {t['task']: {f: med([u[f] for r in task_runs for u in r['tasks'] if u['task'] == t['task']])
                                                  for f in ['ms', 'fuel', 'us_per_fuel']} for t in task_runs[0]['tasks']}
        for mib, runs in worst.items():
            if not runs:
                continue
            x[f'worst_{mib}'] = {
                p['name']: ({'error': p['error']} if 'error' in p else
                            {f: med([q[f] for ps in runs for q in ps if q['name'] == p['name'] and f in q]) for f in
                             ['n', 'ms', 'granted', 'charged', 'us_per_charged', 'slices', 'max_slice_ms', 'max_overdraft', 'max_grant']}
                            | {'wanted': p['wanted']})
                for p in runs[0]}
        return x

    nk = by('ksessions', 'mm:default')
    if nk:
        fu = by('fuel', 'mm:default')
        s['node k'] = ksum(nk)
        s['node fuel'] = fsum([r['programs'] for r in fu if r['what'] == 'programs'],
                              [r for r in fu if r['what'] == 'tasks'],
                              {m: [r['probes'] for r in fu if r['what'] == 'worst' and r['mib'] == m] for m in (16, 256)})
    wk = by('workerd-k', 'mm:default')
    if wk:
        s['workerd k'] = ksum([x for r in wk for x in r['k']])
    wf = by('workerd-fuel', 'mm:default')
    if wf:
        s['workerd fuel'] = fsum([r['programs'] for r in wf], [r['tasks'] for r in wf],
                                 {m: [r[f'worst_{m}'] for r in wf if f'worst_{m}' in r] for m in (16, 256)})
    hibs = [r for r in ok if r['section'] == 'hib']
    s['hib'] = [{'task': r['task'], 'image_bytes': r['start']['image_bytes'], 'open_calls': r['start']['open_calls'],
                 'snapshot_ms': r['start']['snapshot_ms'], 'store_ms': r['start']['store_ms'],
                 'load_ms': r['resume']['load_ms'], 'restore_ms': r['resume']['restore_ms'],
                 'finish_ms': r['resume']['finish_ms'], 'new_isolate': r['start']['isolate'] != r['resume']['isolate'],
                 'resume_ready_ms': r['ready_ms'], 'error': r['resume']['error'], 'output': r['resume']['output']}
                for r in hibs]
    (out / 'summary.json').write_text(json.dumps(s, indent=1) + '\n')
    # The real checker on the first round's calls of every engine and runtime.
    calls = []
    for r in ok:
        if r['section'] == 'workload' and r['round'] == 0:
            calls += [{'engine': f'node {r["engine"]}', 'task': t['task'], 'calls': t['check']['calls'],
                       'error': t['check']['error'] and t['check']['error'][-300:]} for t in r['tasks']]
        if r['section'] == 'workerd' and r['round'] == 0 and 'workload' in r:
            calls += [{'engine': f'workerd {r["engine"]}', 'task': t['task'], 'calls': t['check']['calls'],
                       'error': t['check']['error'] and t['check']['error'][-300:]} for t in r['workload']['tasks']]
        if r['section'] == 'hib' and r['round'] == 0:
            calls.append({'engine': 'workerd DO resume', 'task': r['task'], 'calls': r['resume']['calls'],
                          'error': r['resume']['error']})
    (out / 'calls.jsonl').write_text(''.join(json.dumps(c) + '\n' for c in calls))
    p = subprocess.run([PYTHON, '-I', str(ROOT / 'bench/evidence/engines/check.py'), str(out / 'calls.jsonl')],
                       capture_output=True, text=True)
    (out / 'check.txt').write_text(p.stdout + p.stderr)
    print(json.dumps(s, indent=1)[:3000])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--rounds', type=int, default=5)
    ap.add_argument('--only', default='node,k,workerd,hib')
    ap.add_argument('--out', default=str(HERE / 'results' / time.strftime('%Y-%m-%d')))
    ap.add_argument('--summarize-only', action='store_true')
    a = ap.parse_args()
    out = Path(a.out)
    o = Out(out)
    only = set(a.only.split(','))
    if not a.summarize_only:
        subprocess.run([NODE, 'bench/worker/build.ts'], cwd=JS, check=True, capture_output=True)
        base = Path('/tmp/mm-workerd')
        shutil.rmtree(base, ignore_errors=True)
        o.emit({'section': 'env', 'node': subprocess.run([NODE, '--version'], capture_output=True, text=True).stdout.strip(),
                'workerd': subprocess.run([WORKERD, '--version'], capture_output=True, text=True).stdout.strip(),
                'v8_node': subprocess.run([NODE, '-p', 'process.versions.v8'], capture_output=True, text=True).stdout.strip(),
                'sizes': {e: WASM[e].stat().st_size for e in ALL},
                'git': subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], cwd=ROOT, capture_output=True, text=True).stdout.strip()})
        for rnd in range(a.rounds):
            print(f'round {rnd}', file=sys.stderr)
            if 'node' in only:
                for e in rotate(ALL, rnd):
                    node(o, 'startup', rnd, e, ['startup', '20'])
                    node(o, 'startup', rnd, e, ['startup', '20'], ['--no-wasm-lazy-compilation'])
                    node(o, 'mem', rnd, e, ['mem', '40'])
                    node(o, 'workload', rnd, e, ['workload', '5'])
                    if e in MAIN:
                        node(o, 'hostcall', rnd, e, ['hostcall', 'await', '2000', '5'])
                        node(o, 'hostcall', rnd, e, ['hostcall', 'fanout', '20', '5'])
                        node(o, 'snapshot', rnd, e, ['snapshot'])
                        if rnd == 0:
                            node(o, 'memcap', rnd, e, ['memcap', '32'])
                            node(o, 'memcap', rnd, e, ['memcap', '256'])
            if 'k' in only:
                node_k_fuel(o, rnd)
                workerd_k_fuel(o, rnd, base)
            if 'workerd' in only:
                workerd_round(o, rnd, base)
            if 'hib' in only and rnd < 3:
                hib(o, base, rnd)
    summarize(out)


if __name__ == '__main__':
    main()
