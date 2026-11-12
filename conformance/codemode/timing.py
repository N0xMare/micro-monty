"""Where a codemode cell's time goes: interpreter CPU versus the host boundary.

    python3.14 conformance/codemode/timing.py [--filter SUBSTR] [--jobs N] [--mm BIN]

Runs every census solution (the sets of run.py except the `p` probes) on
CPython (shim.py) and on micro-monty (`mm-codemode`, built in release as
run.py builds `default`, or `--mm BIN`) through run.py's protocol, and
times each run from the harness side:

- `wall`: spawn to exit;
- `cell`: the segments the engine runs (init or answer sent -> next host
  call or the end), process start excluded for the first one;
- `tool`: the mocked tool handlers, in the harness;
- for mm, from the runner's own counters (`timing` in its `done` record):
  `setup` (session, host module, bootstrap cell), `compile` and `engine`
  (time inside `Session::start`/`resume`, i.e. the interpreter running the
  cell plus host-value conversion; it excludes JSON and the pipe), of which
  `start` is the part before the first host call.

A host call's transport cost is then `(cell - (engine - start)) / calls` (runner JSON + pipe + harness decode, the part a
real embedder replaces with its own transport). Prints medians and totals
as Markdown. Mocked tools answer in microseconds; real tools take
milliseconds to seconds, which this does not model.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import ptc  # noqa: E402
import run  # noqa: E402


def timed(command: list[str], task: ptc.Task, mode: str, source: str) -> dict:
    host = ptc.ToolHost(task, mode)
    init = {'source': source, 'tools': [t.name for t in task.tool_defs()], 'fuel': run.FUEL,
            'clock': run.CLOCK}
    env = dict(os.environ, TZ='UTC', PYTHONHASHSEED='0', PYTHONIOENCODING='utf-8')
    t0 = time.perf_counter()
    proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, encoding='utf-8', env=env)
    cell = tool = 0.0
    first = None
    calls = 0
    done = {}
    proc.stdin.write(json.dumps(init) + '\n')
    proc.stdin.flush()
    sent = time.perf_counter()
    for line in proc.stdout:
        got = time.perf_counter()
        if first is None:
            first = got - t0
        else:
            cell += got - sent
        msg = json.loads(line)
        if 'done' in msg:
            done = msg['done']
            break
        args = [ptc.decode(a) for a in msg['args']]
        kwargs = [(k, ptc.decode(v)) for k, v in msg['kwargs']]
        reply = host.handle(msg['name'], args, kwargs)
        calls += 1
        proc.stdin.write(json.dumps(reply) + '\n')
        proc.stdin.flush()
        sent = time.perf_counter()
        tool += sent - got
        if calls > 2000:
            break
    proc.stdin.close()
    proc.wait()
    wall = time.perf_counter() - t0
    out = {'wall': wall, 'first': first or 0.0, 'cell': cell, 'tool': tool, 'calls': calls,
           'ok': not done.get('error')}
    out.update({k[:-3]: v / 1e9 for k, v in done.get('timing', {}).items()})
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--filter')
    ap.add_argument('--jobs', type=int, default=8)
    ap.add_argument('--mm')
    a = ap.parse_args()
    if a.mm:
        mm = a.mm
    else:
        mm = str(run.build()['default'])
    items = [i for i in run.solutions(a.filter) if i[0] != 'p']
    engines = {'cpython': [run.PYTHON, '-I', str(HERE / 'shim.py')], 'mm': [mm]}

    def one(item):
        set_name, task_id, path = item
        source = path.read_text()
        task = ptc.TASKS[task_id]
        rec = {e: timed(c, task, run.SETS[set_name], source) for e, c in engines.items()}
        rec['id'] = f'{set_name}/{task_id}'
        return rec

    with concurrent.futures.ThreadPoolExecutor(a.jobs) as pool:
        recs = list(pool.map(one, items))
    # Runs that pass on both, so both did the same work.
    both = [r for r in recs if r['cpython']['ok'] and r['mm']['ok']]
    ms = lambda xs: statistics.median(xs) * 1e3  # noqa: E731
    print(f'{len(both)} of {len(recs)} solutions without an error on both engines; '
          f'median host calls {statistics.median(r["mm"]["calls"] for r in both):.0f}, '
          f'total {sum(r["mm"]["calls"] for r in both)}\n')
    print('| median ms per solution | CPython 3.14 | micro-monty |')
    print('|---|---:|---:|')
    for key, label in (('wall', 'process wall (spawn to exit)'),
                       ('first', 'spawn to first host call or end'),
                       ('cell', 'cell after the first call (engine + transport)'),
                       ('tool', 'mocked tool handlers (harness)')):
        print(f'| {label} | {ms([r["cpython"][key] for r in both]):.3f} '
              f'| {ms([r["mm"][key] for r in both]):.3f} |')
    for key, label in (('setup', 'mm setup (session, host module, bootstrap)'),
                       ('compile', 'mm compile'),
                       ('engine', 'mm in `start`/`resume` (interpreter + value conversion)')):
        print(f'| {label} | | {ms([r["mm"].get(key, 0) for r in both]):.3f} |')
    tot = {k: sum(r['mm'].get(k, 0) for r in both)
           for k in ('wall', 'engine', 'start', 'setup', 'compile', 'cell', 'tool', 'calls')}
    eng = [r['mm']['engine'] for r in both]
    eng.sort()
    print(f'\nmm totals over {len(both)} runs: wall {tot["wall"]:.2f} s, engine {tot["engine"]:.3f} s '
          f'({100 * tot["engine"] / tot["wall"]:.1f}% of wall), setup {tot["setup"]:.3f} s, '
          f'compile {tot["compile"]:.3f} s')
    print(f'mm engine per run: p50 {ms(eng):.3f} ms, p90 {eng[int(0.9 * len(eng))] * 1e3:.3f} ms, '
          f'p99 {eng[int(0.99 * len(eng))] * 1e3:.3f} ms, max {eng[-1] * 1e3:.3f} ms')
    print('slowest mm cells (engine ms, CPython cell ms after its first call, calls):')
    for r in sorted(both, key=lambda r: -r['mm']['engine'])[:8]:
        print(f'  {r["id"]}: {r["mm"]["engine"] * 1e3:.1f}, {r["cpython"]["cell"] * 1e3:.1f}, '
              f'{r["mm"]["calls"]}')
    calls = tot['calls'] or 1
    print(f'mm per host call: engine-side {1e6 * tot["engine"] / calls:.1f} us of engine time per call '
          f'(upper bound: all engine time / calls), transport {1e6 * (tot["cell"] - tot["engine"] + tot["start"]) / calls:.1f} us')
    return 0


if __name__ == '__main__':
    sys.exit(main())
