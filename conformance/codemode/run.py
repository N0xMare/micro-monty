"""Runs the codemode workload: every solution on CPython and on micro-monty.

    python3.14 conformance/codemode/run.py [--check] [--filter SUBSTR] [--jobs N]

Builds `mm-codemode` twice (`default`: the default stdlib modules;
`full`: plus every optional module), runs each solution under
`solutions/<set>/` on CPython (shim.py) and on both builds, checks the
answers, scans each solution's AST and writes `results/runs.jsonl` and
`results/summary.md`. With `--check` it writes nothing and fails if any
outcome differs from the committed `results/runs.jsonl`. Deterministic and
offline: tools are mocked (tasks/), clocks are pinned and sleeps virtual.
"""

from __future__ import annotations

import argparse
import ast
import builtins
import collections
import concurrent.futures
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))

import ptc  # noqa: E402
import tasks  # noqa: E402,F401  (registers every task)

# 2026-03-15T12:00:00Z, the "today" every prompt states.
CLOCK = 1773576000.0
FUEL = 500_000_000
TIMEOUT = 60
# `a`, `b`, `s`: the first census (one model family, prompt template version 1).
# `luna-*`, `sol-*`: GPT models (`openai/gpt-6-luna`, `openai/gpt-6.1-sol`), template version 2
# (README "How the solutions were written"); the letter after the dash is the prompt style.
# `p`: hand-written probes of error messages, outside the census.
SETS = {'a': 'string', 'b': 'string', 's': 'structured',
        'luna-a': 'string', 'luna-b': 'string', 'luna-s': 'structured',
        'sol-a': 'string', 'sol-b': 'string', 'sol-s': 'structured', 'p': 'string'}
CONFIGS = ('default', 'full')
PYTHON = os.environ.get('PYTHON') or str(Path.home() / '.local/bin/python3.14')

# Module tiers.
DEFAULT_MODULES = {
    'json', 'math', 're', 'collections', 'itertools', 'functools', 'datetime', 'random',
    'asyncio', 'typing', 'base64', 'hashlib', 'dataclasses', 'operator', 'copy', 'heapq',
    'bisect', 'string', 'textwrap', 'enum', 'abc', 'binascii', 'time', 'os', 'pathlib',
    'fnmatch', 'posixpath', 'csv', 'io', 'gc', 'types', 'contextlib', 'traceback',
    'logging', 'urllib', 'pprint', 'shlex', 'difflib', 'struct', 'html', 'warnings',
    'codecs', 'platform', 'sys',
}
OPTIONAL_MODULES = {
    'fractions', 'statistics', 'uuid', 'tomllib', 'decimal', 'numbers', 'contextvars',
    'unicodedata',
}


def tier(module: str) -> str:
    if module in ('html.entities',):
        return 'optional'
    if module in ('html.parser', 'urllib.request', 'urllib.error', 'email', 'xml'):
        return 'unsupported'
    top = module.split('.')[0]
    if module in DEFAULT_MODULES or top in DEFAULT_MODULES and module not in ('collections.abc',):
        return 'default'
    if top in OPTIONAL_MODULES:
        return 'optional'
    return 'unsupported'


# --------------------------------------------------------------------------
# Building


def build() -> dict[str, Path]:
    bins = {}
    for config in CONFIGS:
        target = ROOT / 'target' / 'codemode' / config
        cmd = ['cargo', 'build', '--release', '--locked', '--quiet', '-p', 'mm-codemode',
               '--target-dir', str(target)]
        if config == 'full':
            cmd += ['--features', 'optional-modules']
        subprocess.run(cmd, cwd=ROOT, check=True)
        bins[config] = target / 'release' / 'mm-codemode'
    return bins


# --------------------------------------------------------------------------
# Running


def run_one(command: list[str], task: ptc.Task, mode: str, source: str) -> dict:
    host = ptc.ToolHost(task, mode)
    init = {'source': source, 'tools': [t.name for t in task.tool_defs()], 'fuel': FUEL,
            'clock': CLOCK}
    env = dict(os.environ, TZ='UTC', PYTHONHASHSEED='0', PYTHONIOENCODING='utf-8')
    proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, encoding='utf-8', env=env)
    done = None
    batches = []
    try:
        proc.stdin.write(json.dumps(init) + '\n')
        proc.stdin.flush()
        for line in proc.stdout:
            msg = json.loads(line)
            if 'done' in msg:
                done = msg['done']
                break
            args = [ptc.decode(a) for a in msg['args']]
            kwargs = [(k, ptc.decode(v)) for k, v in msg['kwargs']]
            reply = host.handle(msg['name'], args, kwargs)  # records exactly one call
            batches.append(msg['batch'])
            proc.stdin.write(json.dumps(reply) + '\n')
            proc.stdin.flush()
            if len(host.transcript.calls) > 2000:
                break
    except (BrokenPipeError, json.JSONDecodeError):
        pass
    finally:
        try:
            proc.stdin.close()
        except BrokenPipeError:
            pass
        try:
            proc.wait(timeout=TIMEOUT)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
    if done is None:
        tail = proc.stderr.read()[-500:] if proc.stderr else ''
        done = {'stdout': '', 'stderr': tail, 'calls': len(host.transcript.calls),
                'error': {'type': 'HarnessError', 'message': f'no report (exit {proc.returncode}) {tail}',
                          'traceback': ''}}
    error = done['error']
    if error and error['type'] == 'SystemExit' and error['message'] in ('', '0', 'None'):
        error = None
    t = host.transcript
    t.stdout, t.stderr, t.error = done['stdout'], done['stderr'], error
    problem = None if error else task.check(t)
    return {
        'outcome': 'error' if error else ('pass' if problem is None else 'wrong'),
        'error': error and {'type': error['type'], 'message': portable(error['message'])[:400],
                            'line': _error_line(error)},
        'problem': problem and problem[:400],
        'calls': [[c.name, c.args, c.error] for c in t.calls],
        'batches': batches,
        'stdout': t.stdout[-2000:],
        'stdout_bytes': len(t.stdout.encode()),
        'max_waiting': done.get('max_waiting'),
    }


def call_batches(record: dict) -> list[list[str]]:
    """The tool calls (name, arguments, error) as consecutive batches, each sorted: calls issued while the
    cell waits on the same batch (one `asyncio.gather`'s fan-out) may run in any order, but batches keep theirs."""
    out: list[list[str]] = []
    last = None
    for call, batch in zip(record['calls'], record['batches'], strict=True):
        if not out or batch != last:
            out.append([])
        out[-1].append(json.dumps(call, sort_keys=True))
        last = batch
    return [sorted(b) for b in out]


def _error_line(error: dict) -> str:
    """The last `solution.py` source line of a traceback."""
    lines = error.get('traceback', '').splitlines()
    for i in range(len(lines) - 1, -1, -1):
        if 'solution.py' in lines[i]:
            for nxt in lines[i + 1:i + 2]:
                return portable(nxt.strip())
    return ''


# A CPython stdlib file in a traceback: `"/…/lib/python3.14/json/__init__.py"`.
_STDLIB = re.compile(r'"[^"]*/lib/python3\.\d+/')


def portable(text: str) -> str:
    """`text` with host paths as placeholders: the checkout as `<repo>` and
    the interpreter's stdlib as `<stdlib>`, so recorded results do not
    depend on where the checkout or CPython is installed."""
    return _STDLIB.sub('"<stdlib>/', text.replace(str(ROOT), '<repo>'))


# --------------------------------------------------------------------------
# Static scan

BUILTIN_NAMES = set(dir(builtins))


def scan(source: str) -> dict:
    try:
        tree = ast.parse(source)
    except SyntaxError as e:
        return {'imports': [], 'builtins': [], 'features': [], 'attrs': [], 'parse_error': str(e)}
    imports, attrs, features = set(), set(), set()
    aliases: dict[str, str] = {}
    stored = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for a in node.names:
                imports.add(a.name)
                aliases[(a.asname or a.name).split('.')[0]] = a.name if a.asname else a.name.split('.')[0]
        elif isinstance(node, ast.ImportFrom) and node.module and not node.level:
            imports.add(node.module)
            for a in node.names:
                attrs.add(f'{node.module}.{a.name}')
        elif isinstance(node, ast.Name) and isinstance(node.ctx, ast.Store):
            stored.add(node.id)
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            stored.add(node.name)
        elif isinstance(node, ast.arg):
            stored.add(node.arg)
    for node in ast.walk(tree):
        if isinstance(node, ast.Attribute):
            chain, cur = [node.attr], node.value
            while isinstance(cur, ast.Attribute):
                chain.append(cur.attr)
                cur = cur.value
            if isinstance(cur, ast.Name) and cur.id in aliases:
                chain.append(aliases[cur.id])
                chain.reverse()
                attrs.add('.'.join(chain[:3]))
        features.update(_features(node))
    used = sorted({n.id for n in ast.walk(tree)
                   if isinstance(n, ast.Name) and isinstance(n.ctx, ast.Load)
                   and n.id in BUILTIN_NAMES and n.id not in stored})
    return {'imports': sorted(imports), 'builtins': used, 'features': sorted(features),
            'attrs': sorted(attrs)}


FEATURE_NODES = {
    ast.Match: 'match', ast.NamedExpr: 'walrus', ast.AsyncFor: 'async for',
    ast.AsyncWith: 'async with', ast.AsyncFunctionDef: 'async def', ast.Await: 'await',
    ast.Lambda: 'lambda', ast.ListComp: 'listcomp', ast.DictComp: 'dictcomp',
    ast.SetComp: 'setcomp', ast.GeneratorExp: 'genexp', ast.Try: 'try', ast.TryStar: 'except*',
    ast.ClassDef: 'class', ast.Yield: 'yield', ast.YieldFrom: 'yield from',
    ast.Global: 'global', ast.Nonlocal: 'nonlocal', ast.With: 'with', ast.Assert: 'assert',
    ast.Raise: 'raise', ast.Starred: 'starred', ast.Delete: 'del', ast.AnnAssign: 'annotation',
    ast.JoinedStr: 'f-string', ast.TypeAlias: 'type alias',
}


def _features(node: ast.AST) -> set[str]:
    out = set()
    name = FEATURE_NODES.get(type(node))
    if name:
        out.add(name)
    if isinstance(node, ast.FormattedValue) and node.format_spec is not None:
        out.add('format spec')
    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) and node.decorator_list:
        out.add('decorator')
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Mod) and isinstance(node.left, ast.Constant) \
            and isinstance(node.left.value, str):
        out.add('%-format')
    return out


# --------------------------------------------------------------------------
# Failure causes

# (regex over "Type: message | line", cause label); first match wins.
CAUSES: list[tuple[str, str]] = []


def cause(record: dict) -> str | None:
    """The grouped first failure cause of one engine run."""
    if record['outcome'] == 'pass':
        return None
    if record['outcome'] == 'wrong':
        return 'wrong output: ' + _shape(record['problem'] or '')
    e = record['error']
    text = f"{e['type']}: {e['message']} | {e['line']}"
    for pattern, label in CAUSES:
        if re.search(pattern, text):
            return label
    if e['type'] == 'ModuleNotFoundError':
        m = re.search(r"No module named '([^']+)'", e['message'])
        return f"import {m.group(1) if m else e['message']}"
    return f"{e['type']}: {_shape(e['message'])}"


def _shape(message: str) -> str:
    message = re.sub(r"'[^']*'", "'…'", message)
    message = re.sub(r'\d+', 'N', message)
    return message[:120]


# --------------------------------------------------------------------------
# Main


def solutions(filter_: str | None) -> list[tuple[str, str, Path]]:
    out = []
    for set_name in SETS:
        for path in sorted((HERE / 'solutions' / set_name).glob('*.py')):
            task_id = path.stem
            if task_id not in ptc.TASKS:
                raise SystemExit(f'{path}: no task {task_id}')
            if filter_ and filter_ not in f'{set_name}/{task_id}':
                continue
            out.append((set_name, task_id, path))
    return out


def run_all(bins: dict[str, Path], items, jobs: int) -> list[dict]:
    engines = {'cpython': [PYTHON, '-I', str(HERE / 'shim.py')]}
    engines.update({f'mm-{c}': [str(p)] for c, p in bins.items()})

    def one(item):
        set_name, task_id, path = item
        source = path.read_text()
        task = ptc.TASKS[task_id]
        mode = SETS[set_name]
        rec = {'set': set_name, 'task': task_id, 'category': task.category, 'mode': mode}
        rec.update(scan(source))
        for engine, cmd in engines.items():
            rec[engine] = run_one(cmd, task, mode, source)
        ref = rec['cpython']
        for engine in engines:
            if engine != 'cpython':
                r = rec[engine]
                r['cause'] = cause(r)
                r['same_calls'] = call_batches(r) == call_batches(ref)
                r['same_stdout'] = r['stdout'] == ref['stdout']
        rec['cpython']['cause'] = cause(ref)
        return rec

    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        return list(pool.map(one, items))


def compact(rec: dict) -> dict:
    """What runs.jsonl keeps (and --check compares)."""
    out = {k: rec[k] for k in ('set', 'task', 'category', 'mode', 'imports', 'builtins', 'features', 'attrs')}
    for engine in ('cpython', 'mm-default', 'mm-full'):
        r = rec[engine]
        keys = ('outcome', 'cause', 'error', 'problem', 'same_calls', 'same_stdout', 'stdout_bytes', 'max_waiting')
        out[engine] = {k: r.get(k) for k in keys if r.get(k) is not None}
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--check', action='store_true')
    ap.add_argument('--filter')
    ap.add_argument('--jobs', type=int, default=os.cpu_count() or 4)
    ap.add_argument('--verbose', action='store_true', help='print every non-pass in full')
    args = ap.parse_args()
    bins = build()
    items = solutions(args.filter)
    records = [compact(r) for r in run_all(bins, items, args.jobs)]
    if args.verbose:
        for r in records:
            for engine in ('cpython', 'mm-default', 'mm-full'):
                if r[engine]['outcome'] != 'pass':
                    print(f"{r['set']}/{r['task']} {engine}: {json.dumps(r[engine])}")
    results = HERE / 'results'
    lines = [json.dumps(r, sort_keys=True) for r in records]
    import summary  # noqa: PLC0415
    text = summary.render(records, {c: _sha256(p) for c, p in bins.items()})
    if args.check:
        committed = (results / 'runs.jsonl').read_text().splitlines()
        if args.filter:
            keys = {(r['set'], r['task']) for r in records}
            committed = [line for line in committed
                         if (json.loads(line)['set'], json.loads(line)['task']) in keys]
        if committed != lines:
            old = {(json.loads(x)['set'], json.loads(x)['task']): x for x in committed}
            for line in lines:
                r = json.loads(line)
                if old.get((r['set'], r['task'])) != line:
                    print(f"changed: {r['set']}/{r['task']}")
            print('codemode: results differ from results/runs.jsonl')
            return 1
        print(f'codemode: {len(lines)} solutions match results/runs.jsonl')
        return 0
    if not args.filter:
        results.mkdir(exist_ok=True)
        (results / 'runs.jsonl').write_text('\n'.join(lines) + '\n')
        (results / 'summary.md').write_text(text)
    print(text.split('\n## ', 2)[1] if '\n## ' in text else text)
    return 0


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


if __name__ == '__main__':
    sys.exit(main())
