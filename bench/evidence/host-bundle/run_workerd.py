"""The bundle guest against the default guest: stages.ts in workerd, per guest.

    python3 run_workerd.py N GUEST.wasm...
    CORE=3 python3 run_workerd.py --times ROUNDS GUEST.wasm...

Bundles worker.ts with hosts/js's esbuild, serves each guest as the Worker's
`engine.wasm` (a compiled module, as Wrangler's CompiledWasm rule does) and
prints one JSON line per guest: workerd RSS and linear memory per session idle,
after a cell and after importing json, re and collections, and the times.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
JS = HERE.parents[2] / 'hosts/js'
PORT = 18787
CONFIG = '''using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
  services = [ (name = "main", worker = .w) ],
  sockets = [ (name = "http", address = "127.0.0.1:{port}", http = (), service = "main") ],
);
const w :Workerd.Worker = (
  modules = [
    (name = "worker.js", esModule = embed "worker.js"),
    (name = "engine.wasm", wasm = embed "engine.wasm"),
  ],
  compatibilityDate = "2026-11-01",
);
'''


def get(path):
    with urllib.request.urlopen(f'http://127.0.0.1:{PORT}{path}', timeout=600) as r:
        body = json.loads(r.read())
    if 'error' in body:
        raise RuntimeError(body['error'])
    return body


def rss(pid):
    for line in Path(f'/proc/{pid}/status').read_text().splitlines():
        if line.startswith('VmRSS:'):
            return int(line.split()[1]) * 1024
    return 0


def serve(work, guest):
    """A workerd serving `guest`, pinned to one core when `CORE` is set."""
    shutil.copy(guest, work / 'engine.wasm')
    pin = ['taskset', '-c', os.environ['CORE']] if os.environ.get('CORE') else []
    p = subprocess.Popen([*pin, JS / 'node_modules/.bin/workerd', 'serve', 'config.capnp'], cwd=work,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    while True:
        try:
            get('/ping')
            return p
        except OSError:
            time.sleep(0.005)


def batch(m, reps):
    """Median of 5 timed batches of `reps` fresh sessions, µs per session."""
    best = []
    for _ in range(5):
        t = time.perf_counter()
        get(f'/batch?m={m}&reps={reps}')
        best.append((time.perf_counter() - t) / reps * 1e6)
    return sorted(best)[2]


def time_rounds(rounds, guests):
    """`--times ROUNDS`: first-import times only, guests alternating within
    each round (a fresh workerd per guest and round), so drift hits both."""
    work = bundle_worker()
    mods = ('json', 're', 'collections', 'datetime')
    res = {g: {m: [] for m in ('base', *mods)} for g in guests}
    for r in range(rounds):
        for g in (guests if r % 2 == 0 else guests[::-1]):
            p = serve(work, g)
            try:
                get('/batch?m=datetime&reps=5')
                base = batch('', 50)
                res[g]['base'].append(base)
                for m in mods:
                    res[g][m].append(batch(m, 50) - base)
            finally:
                p.kill()
                p.wait()
    print(json.dumps({'rounds': rounds, 'us_per_round': res}))


def bundle_worker():
    work = Path(tempfile.mkdtemp())
    subprocess.run([JS / 'node_modules/.bin/esbuild', HERE / 'worker.ts', '--bundle', '--format=esm',
                    '--platform=neutral', '--target=es2024', '--external:./engine.wasm',
                    f'--outfile={work / "worker.js"}', '--log-level=warning'], check=True)
    (work / 'config.capnp').write_text(CONFIG.format(port=PORT))
    return work


def main():
    if sys.argv[1] == '--times':
        return time_rounds(int(sys.argv[2]), sys.argv[3:])
    n = int(sys.argv[1])
    work = Path(tempfile.mkdtemp())
    subprocess.run([JS / 'node_modules/.bin/esbuild', HERE / 'worker.ts', '--bundle', '--format=esm',
                    '--platform=neutral', '--target=es2024', '--external:./engine.wasm',
                    f'--outfile={work / "worker.js"}', '--log-level=warning'], check=True)
    (work / 'config.capnp').write_text(CONFIG.format(port=PORT))
    for guest in sys.argv[2:]:
        shutil.copy(guest, work / 'engine.wasm')
        t = time.perf_counter()
        p = subprocess.Popen([JS / 'node_modules/.bin/workerd', 'serve', 'config.capnp'], cwd=work,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            while True:
                try:
                    get('/ping')
                    break
                except OSError:
                    time.sleep(0.005)
            ready_ms = (time.perf_counter() - t) * 1000
            get('/add?n=1'), get('/run?cell=imports'), get('/clear')
            time.sleep(0.2)
            base = rss(p.pid)
            per = {}
            lin = {}
            for stage, path in (('idle', f'/add?n={n}'), ('cell', '/run?cell=cell'), ('imports', '/run?cell=imports')):
                r = get(path)
                time.sleep(0.2)
                per[stage] = round((rss(p.pid) - base) / n / 1024)
                lin[stage] = round(r['linear'] / n / 1024)
            get('/clear')
            def batch(m, reps=50):
                best = []
                for _ in range(5):
                    t = time.perf_counter()
                    get(f'/batch?m={m}&reps={reps}')
                    best.append((time.perf_counter() - t) / reps * 1e6)
                return sorted(best)[2]
            get('/batch?m=datetime&reps=5')
            base_us = batch('')
            times = {'session_and_first_cell': base_us}
            for m in ('json', 're', 'collections', 'datetime'):
                times[f'import_{m}'] = batch(m) - base_us
            print(json.dumps({'guest': guest, 'n': n, 'ready_ms': round(ready_ms, 1), 'rss_kib_per_session': per,
                              'linear_kib_per_session': lin, 'us': {k: round(v) for k, v in times.items()}}))
        finally:
            p.kill()
            p.wait()


main()
