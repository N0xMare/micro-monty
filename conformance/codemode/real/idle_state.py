#!/usr/bin/env python3
"""What a session's globals hold between cells: the idle-state census (UPG-1).

    python3.14 conformance/codemode/real/idle_state.py            # rewrite results/idle-state.md
    python3.14 conformance/codemode/real/idle_state.py --check    # fail if the census moved

Recorded sessions: each py-ptc session of `traces.tar.xz` is replayed as
`gate.py` replays it (same worker, frames and recorded nested results), and
after every cell a probe cell classifies everything reachable from
`globals()`: the state the next cell starts from (the last cell's, before
`Shutdown`). py-sdk
sessions are left out: plak resets their session before every cell. The
probe defines one function, prints one line and deletes the function, so
the next recorded cell sees the globals it would have seen; the replay
checks that every recorded cell still gives what `results/gate.jsonl`
expects.

Synthetic solutions: each codemode solution (`../solutions/`, sets `a` to
`sol-s`) runs once more on the default `mm-codemode` build with the probe
appended: the state a host would carry after it.

The probe follows lists, tuples, sets, frozensets and dicts (keys and
values) and classifies each other value it reaches: plain data (None,
bool, numbers, str, bytes, bytearray, range, slice), a module, a top-level
function of the session (`def`), an imported function or class
(`import`), a stdlib instance (by its type), a coroutine, generator or
async generator, or a user class, user instance, closure, lambda, bound
method or task. A state is counted under the worst kind it holds, in this
order: user objects, then coroutines and generators, then stdlib
instances, then plain data, imports and defs; a state with no name at all
is `nothing`. The tool functions the bootstrap defines are not counted.
The probe cannot tell a finished coroutine or generator from a live one.
"""

from __future__ import annotations

import argparse
import ast
import collections
import concurrent.futures
import json
import os
import subprocess
import sys
import threading
import warnings
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))

import carry  # noqa: E402
import gate  # noqa: E402

OUT = HERE / 'results' / 'idle-state.md'
MARK = '\x1eIDLE '

PROBE = '''
def __mm_idle_probe(skip):
    plain = (type(None), bool, int, float, complex, str, bytes, bytearray, range, slice)
    out = {}
    g = globals()
    for name in list(g):
        if name in skip or name[:2] == '__' and name[-2:] == '__' or name[:5] == '__mm_':
            continue
        kinds = set()
        todo = [g[name]]
        seen = set()
        while todo:
            v = todo.pop()
            if id(v) in seen:
                continue
            seen.add(id(v))
            t = type(v)
            n = t.__name__
            if t in plain:
                kinds.add('data')
            elif t is list or t is tuple or t is set or t is frozenset:
                kinds.add('data')
                todo.extend(v)
            elif t is dict:
                kinds.add('data')
                todo.extend(v.keys())
                todo.extend(v.values())
            elif n == 'module':
                kinds.add('module')
            elif n in ('coroutine', 'generator', 'async_generator'):
                kinds.add(n)
            elif isinstance(v, type):
                kinds.add('class' if v.__module__ == '__main__' else 'import')
            elif n == 'function':
                if v.__name__ == '<lambda>':
                    kinds.add('lambda')
                elif v.__module__ != '__main__':
                    kinds.add('import')
                elif '<locals>' in v.__qualname__ or getattr(v, '__closure__', None):
                    kinds.add('closure')
                else:
                    kinds.add('def')
            elif n in ('builtin_function_or_method', 'method_descriptor'):
                kinds.add('import')
            elif n == 'method':
                kinds.add('method')
            elif n in ('Task', 'Future'):
                kinds.add('task')
            elif t.__module__ == '__main__':
                kinds.add('instance')
            else:
                kinds.add('stdlib ' + t.__module__ + '.' + t.__qualname__)
        out[name] = sorted(kinds)
    print(%r + repr(out))
__mm_idle_probe(%r)
del __mm_idle_probe
'''

USER = ('class', 'instance', 'closure', 'lambda', 'method', 'task')
SUSPENDED = ('coroutine', 'generator', 'async_generator')
ROWS = ('nothing', 'only plain data, imports and top-level defs', 'also stdlib instances',
        'a coroutine or generator object', 'a user class or instance, closure, lambda or task')


def probe(skip) -> str:
    return PROBE % (MARK, sorted(skip))


def row(state: dict) -> int:
    """The census row of one probed state."""
    if not state:
        return 0
    kinds = {k for ks in state.values() for k in ks}
    if kinds & set(USER):
        return 4
    if kinds & set(SUSPENDED):
        return 3
    if any(k.startswith('stdlib ') for k in kinds):
        return 2
    return 1


def parse_probe(output: str) -> dict | None:
    for line in output.split("\n"):
        if line.startswith(MARK):
            return ast.literal_eval(line[len(MARK):])
    return None


def reads(source: str) -> set[str]:
    """Names a cell loads that it never binds itself."""
    try:
        with warnings.catch_warnings():
            warnings.simplefilter('ignore')
            tree = ast.parse(source)
    except SyntaxError:
        return set()
    loaded, bound = set(), set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Name):
            (loaded if isinstance(node.ctx, ast.Load) else bound).add(node.id)
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            bound.add(node.name)
        elif isinstance(node, (ast.Import, ast.ImportFrom)):
            bound.update((a.asname or a.name).split('.')[0] for a in node.names)
        elif isinstance(node, ast.arg):
            bound.add(node.arg)
    return loaded - bound


def probe_session(path, session, expected):
    """Replays one session with a probe before every cell but the first."""
    sid, clock, lines = session
    script, cells = gate.recorded_cells(lines)
    skip = carry.bootstrap_of(script)[1]
    worker = gate.Worker(path, clock)
    states, moved, broken = [], [], False
    try:
        if (worker.recv() or {}).get('frame') != 'hello':
            raise SystemExit(f'{sid}: no hello from {path}')
        last = None

        def probe_after(cell, nxt):
            start = {'frame': 'start_cell', 'cell': 1_000_000 + cell, 'source': probe(skip),
                     'prelude': {'engine': 'monty', 'bootstrap': None}, 'window': None}
            seen, diverged = gate.replay_cell(worker, start, {'answers': []})
            state = parse_probe(seen['output'])
            if diverged or state is None or seen['outcome'].get('outcome') != 'completed':
                raise SystemExit(f'{sid}/{cell}: the probe failed: {diverged or seen}')
            states.append((cell, state, reads(nxt) if nxt is not None else None))

        for frame in script:
            if frame['frame'] == 'shutdown' and last is not None:
                probe_after(last, None)
                last = None
            if frame['frame'] != 'start_cell':
                worker.send(frame)
                continue
            cell = frame['cell']
            if last is not None:
                probe_after(last, frame['source'])
            last = cell
            seen, diverged = gate.replay_cell(worker, frame, cells[cell])
            want = expected.get(f'{sid}/{cell}', {})
            for field in gate.FIELDS:
                if seen[field] != want.get(field, cells[cell][field]):
                    moved.append(f'{sid}/{cell} {field}')
            if diverged:
                broken = True
                break
        if broken:
            worker.process.kill()
        worker.process.stdin.close()
        worker.process.wait(timeout=gate.TIMEOUT)
    finally:
        if worker.process.poll() is None:
            worker.process.kill()
    return sid, states, moved


def recorded(jobs: int):
    expected = {}
    for line in gate.EXPECTED.read_text().splitlines():
        record = json.loads(line)
        expected[record['cell']] = record
    path = gate.build()
    todo = [s for s in gate.sessions(None) if '-ptc-' in s[0]]
    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        return sorted(pool.map(lambda s: probe_session(path, s, expected), todo))


def synthetic(jobs: int):
    import run as codemode  # noqa: PLC0415  (conformance/codemode/run.py)
    import ptc  # noqa: PLC0415
    target = codemode.ROOT / 'target' / 'codemode' / 'default'
    subprocess.run(['cargo', 'build', '--release', '--locked', '--quiet', '-p', 'mm-codemode',
                    '--target-dir', str(target)], cwd=codemode.ROOT, check=True)
    command = [str(target / 'release' / 'mm-codemode')]
    # The tools share one sqlite connection (tasks/worlds.py): one tool call at a time.
    lock = threading.Lock()
    handle = ptc.ToolHost.handle

    def locked(self, *args):
        with lock:
            return handle(self, *args)

    ptc.ToolHost.handle = locked
    items = [i for i in codemode.solutions(None) if i[0] != 'p']

    def one(item):
        set_name, task_id, path = item
        task = ptc.TASKS[task_id]
        skip = {t.name for t in task.tool_defs()} | {'_ptc'}
        source = path.read_text() + '\n' + probe(skip)
        result = codemode.run_one(command, task, codemode.SETS[set_name], source)
        return f'{set_name}/{task_id}', parse_probe(result['stdout'])

    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        return list(pool.map(one, items))


def table(states: list[dict]) -> list[str]:
    counts = collections.Counter(row(s) for s in states)
    total = len(states)
    out = ['| What globals hold between cells | States |', '|---|---:|']
    for i, label in enumerate(ROWS):
        out.append(f'| {label} | {counts[i]} ({100 * counts[i] / total:.1f}%) |')
    return out


def kinds_table(states: list[dict], title: str) -> list[str]:
    """How many states hold each kind other than plain data, modules and defs."""
    counts = collections.Counter()
    for s in states:
        for k in {k for ks in s.values() for k in ks}:
            if k not in ('data', 'module', 'def', 'import'):
                counts[k] += 1
    if not counts:
        return [f'{title}: none.']
    out = [f'| {title} | States |', '|---|---:|']
    out += [f'| `{k}` | {n} |' for k, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))]
    return out


def report(sessions, solutions) -> str:
    states = [s for _, ss, _ in sessions for _, s, _ in ss]
    by_model = collections.defaultdict(list)
    for sid, ss, _ in sessions:
        by_model[sid.split('/')[1].split('-')[0]].extend(s for _, s, _ in ss)
    moved = [m for _, _, ms in sessions for m in ms]
    # Cells that read a name an earlier cell left, by the kind of what they read.
    readers, reader_sessions, read_kinds = 0, set(), collections.Counter()
    for sid, ss, _ in sessions:
        for _, state, names in ss:
            hit = names & set(state) if names is not None else set()
            if hit:
                readers += 1
                reader_sessions.add(sid)
                for k in {k for n in hit for k in state[n]}:
                    read_kinds[k.split(' ')[0]] += 1
    lines = [
        '# Idle state between cells',
        '',
        'Generated by `idle_state.py` (the method is in its docstring). The probe ran after every',
        'py-ptc cell, on the state the next cell starts from, as replayed on current mm.',
        '',
        f'## Recorded sessions: {len(sessions)} py-ptc sessions, {len(states)} between-cell states',
        '',
        *table(states),
        '',
        *(f'- {model}: {len(ss)} states; ' + ', '.join(
            f'{ROWS[i]} {n}' for i, n in sorted(collections.Counter(row(s) for s in ss).items()))
          for model, ss in sorted(by_model.items())),
        '',
        *kinds_table(states, 'Kinds beyond plain data, imports and defs'),
        '',
        f'Cells that read a name an earlier cell left (static: loaded and never bound in the cell): '
        f'{readers} of {len(states) - len(sessions)} cells after the first, in {len(reader_sessions)} of {len(sessions)} sessions; by '
        'what they read: ' + (', '.join(f'{k} {n}' for k, n in sorted(read_kinds.items())) or 'none') + '.',
        '',
        f'Recorded cells whose replay moved with the probes in place: {len(moved)}'
        + (' (' + ', '.join(moved[:10]) + ')' if moved else '') + '.',
        '',
    ]
    probed = [s for _, s in solutions if s is not None]
    lines += [
        f'## Synthetic solutions: {len(probed)} of {len(solutions)} reached the probe',
        '',
        'The state each one-shot solution leaves (default `mm-codemode` build); a solution that',
        'raises never reaches the probe.',
        '',
        *table(probed),
        '',
        *kinds_table(probed, 'Kinds beyond plain data, imports and defs'),
        '',
    ]
    return '\n'.join(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--check', action='store_true', help='write nothing; fail if the census moved')
    ap.add_argument('--jobs', type=int, default=os.cpu_count() or 4)
    args = ap.parse_args()
    text = report(recorded(args.jobs), synthetic(args.jobs))
    if args.check:
        if OUT.read_text() != text:
            print(text)
            print(f'FAIL: the census differs from {OUT.relative_to(gate.ROOT)}')
            return 1
        print('idle-state: the census is unchanged')
        return 0
    OUT.write_text(text)
    print(text)
    return 0


if __name__ == '__main__':
    sys.exit(main())
