"""Records the replay fixtures of the codemode workload (README "Codemode workload").

    python3.14 bench/evidence/engines/record.py

For each task in TASKS, runs the structured-results solution of set `s`
(conformance/codemode/solutions/s/<task>.py) on CPython through
conformance/codemode/shim.py with the real mocked tools (ToolHost), and writes
fixtures/<task>.json: the tool definitions and every call with its reply, in
order. The engine hosts answer each call from this table, keyed by tool name and
canonical JSON arguments, so every engine sees the same tool results and no
engine pays for a Python mock. `check.py` runs the real checker over the calls an
engine made.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
CODEMODE = ROOT / 'conformance' / 'codemode'
sys.path.insert(0, str(CODEMODE))

import ptc  # noqa: E402
import tasks  # noqa: E402,F401  (registers every task)

PYTHON = os.environ.get('PYTHON') or str(Path.home() / '.local/bin/python3.14')
CLOCK = 1773576000.0

# About 20 tasks over aggregate, paginate, batch (fan-out), retry, filter and join.
TASKS = [
    # aggregate
    'agg-avg-order-by-tier', 'agg-customer-summary', 'agg-top-customers', 'agg-revenue-by-region',
    # paginate
    'page-all-pending', 'page-open-bugs', 'page-accounts-offset', 'page-feb-orders',
    # batch / fan-out
    'par-two-level-fanout', 'par-weather-warmest', 'neg-fanout-40', 'rep-quotes-csv',
    # retry
    'retry-gather-flaky', 'retry-give-up', 'retry-permanent-errors', 'retry-backoff',
    # filter / join
    'logs-access-top-ips', 'logs-errors-per-service', 'wf-close-fixed', 'reshape-nested-counts',
    'wf-customer-latest-order', 'stat-max-drawdown',
]


def plain(value):
    """A decoded tool value as plain JSON (tuples become lists)."""
    if isinstance(value, dict):
        return {str(k): plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [plain(v) for v in value]
    return value


def record(task_id: str) -> dict:
    task = ptc.TASKS[task_id]
    host = ptc.ToolHost(task, 'structured')
    source = (CODEMODE / 'solutions' / 's' / f'{task_id}.py').read_text()
    init = {'source': source, 'tools': [t.name for t in task.tool_defs()], 'clock': CLOCK}
    env = dict(os.environ, TZ='UTC', PYTHONHASHSEED='0', PYTHONIOENCODING='utf-8')
    proc = subprocess.Popen([PYTHON, str(CODEMODE / 'shim.py')], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, text=True, encoding='utf-8', env=env)
    proc.stdin.write(json.dumps(init) + '\n')
    proc.stdin.flush()
    calls = []
    done = None
    for line in proc.stdout:
        msg = json.loads(line)
        if 'done' in msg:
            done = msg['done']
            break
        args = [ptc.decode(a) for a in msg['args']]
        reply = host.handle(msg['name'], args, [])
        entry = {'name': msg['name'], 'args': plain(args[0])}
        if 'ok' in reply:
            entry['ok'] = plain(ptc.decode(reply['ok']))
        else:
            entry['err'] = reply['err']
        calls.append(entry)
        proc.stdin.write(json.dumps(reply) + '\n')
        proc.stdin.flush()
    proc.stdin.close()
    proc.wait()
    assert done is not None and done['error'] is None, (task_id, done)
    t = host.transcript
    t.stdout = done['stdout']
    problem = task.check(t)
    assert problem is None, (task_id, problem)
    return {
        'task': task_id,
        'category': task.category,
        'tools': [{'name': d.name, 'description': d.description, 'params': list(d.params)}
                  for d in task.tool_defs()],
        'calls': calls,
        'stdout': done['stdout'],
    }


def main() -> None:
    out = HERE / 'fixtures'
    out.mkdir(exist_ok=True)
    for task_id in TASKS:
        fixture = record(task_id)
        (out / f'{task_id}.json').write_text(json.dumps(fixture, indent=1, sort_keys=True) + '\n')
        print(f'{task_id}: {len(fixture["calls"])} calls')


if __name__ == '__main__':
    main()
