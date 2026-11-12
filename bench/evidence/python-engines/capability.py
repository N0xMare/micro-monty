"""Capability runs of docs/research/python-engines-compare.md: the Monty
corpus, mm's own cases and the codemode solutions on MicroPython and PocketPy
(and the same runners on mm, for one table), judged against CPython 3.14.

    python3.14 capability.py corpus|own|codemode [--engines micropython,pocketpy,mm] --out DIR

- corpus: conformance/oracle.py's micro-monty side, unchanged, with
  `adapters/pyexec.py` standing in for `mm exec --json`. `call-external`
  cases need the datatest's host fixtures, which only mm emulates, so they
  are reported apart (`host` tag); the comparison is the `cpython` tag.
- own: each conformance/own case as a script on the engine's CLI against
  CPython (`-X utf8`, PYTHONHASHSEED=0, TZ=UTC): stdout and exit status must
  match (`pass`); `strict` also needs identical stderr (tracebacks).
- codemode: every solution under conformance/codemode/solutions through
  conformance/codemode/run.py's `run_one` (the real mocked tools and
  checkers), with codemode/<engine>_shim.py speaking the runner protocol
  inside the engine. CPython's outcomes are the committed
  results/runs.jsonl.

Every record gets a first cause (`cause`): syntax, missing module, missing
builtin, missing attribute, not implemented, traceback format, wrong result
(semantics), crash, timeout.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import re
import resource
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PENG = Path(os.environ.get('PENG', '/tmp/peng'))
PYTHON = os.environ.get('PYTHON') or str(Path.home() / '.local/bin/python3.14')
MM = ROOT / 'target/release/mm'
BIN = {'micropython': PENG / 'bin/micropython', 'pocketpy': PENG / 'bin/pocketpy'}
JOBS = int(os.environ.get('JOBS', max(1, (os.cpu_count() or 2) - 2)))
BUILTINS = set(dir(__import__('builtins')))


def engine_env(engine: str) -> dict:
    return dict(os.environ, TZ='UTC', PYTHONHASHSEED='0', MICROPYPATH=f'.frozen:{PENG}/mpylib')


def adapter(engine: str) -> Path:
    """A runner for the oracle with absolute paths baked in: the oracle may
    pass it an emptied environment (pyexec.py's doc)."""
    d = PENG / 'adapters'
    d.mkdir(parents=True, exist_ok=True)
    p = d / f'pyexec-{engine}'
    p.write_text(f'#!/bin/sh\nPENG={PENG} exec {PYTHON} -I {HERE}/adapters/pyexec.py {engine} "$@"\n')
    p.chmod(0o755)
    return p


EXC_LINE = re.compile(r'^(\w+(?:Error|Exception|Exit|Interrupt|Iteration|Warning)?)(?::\s*(.*))?$')


def classify(error_type: str | None, message: str, kind: str) -> str:
    """The grouped first cause of a failure."""
    t, m = error_type or '', message or ''
    if t == 'SyntaxError' or 'SyntaxError' in kind:
        return 'syntax'
    if t in ('ImportError', 'ModuleNotFoundError'):
        mod = re.search(r"(?:module named|named) '?([\w.]+)'?", m)
        return 'missing module' + (f' {mod.group(1)}' if mod else '')
    if t == 'NameError':
        name = re.search(r"name '(\w+)'", m) or re.search(r'(\w+)', m)
        if name and name.group(1) in BUILTINS:
            return 'missing builtin'
    if t == 'AttributeError' and re.search(r"has no attribute|no attribute", m):
        return 'missing attribute or method'
    if t == 'NotImplementedError':
        return 'not implemented'
    return 'semantics'


# ---------------------------------------------------------------------------
# Corpus


def corpus(engine: str, out: Path) -> None:
    cases = sorted((ROOT / 'conformance/monty').glob('*.py'))
    tags = {json.loads(ln)['file']: json.loads(ln)['tags']
            for ln in (ROOT / 'conformance/results/micro-monty.jsonl').read_text().splitlines()}
    runner = str(MM) if engine == 'mm' else str(adapter(engine))

    def one(path: Path) -> dict:
        p = subprocess.run([PYTHON, str(ROOT / 'conformance/oracle.py'), '--engine', 'micro-monty', '--mm', runner,
                            str(path)], capture_output=True, text=True, timeout=600)
        try:
            rec = json.loads(p.stdout.strip().splitlines()[-1])
        except (IndexError, ValueError):
            rec = {'file': path.name, 'status': 'error', 'reason': p.stderr[-300:]}
        rec['tags'] = tags.get(path.name, [])
        header = rec.get('header') or {}
        if engine != 'mm' and header.get('call-external') and rec['status'] != 'skip':
            rec['status'], rec['reason'] = 'skip', 'needs the datatest host fixtures (mm only)'
        rec['cause'] = corpus_cause(rec)
        for k in ('expected', 'actual'):
            if isinstance(rec.get(k), str):
                rec[k] = rec[k][-600:]
        return rec

    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        recs = list(pool.map(one, cases))
    with (out / f'corpus-{engine}.jsonl').open('w') as f:
        for r in recs:
            f.write(json.dumps(r) + '\n')


def corpus_cause(rec: dict) -> str | None:
    st = rec['status']
    if st in ('pass', 'skip'):
        return None
    if st in ('crash', 'timeout', 'error'):
        return st
    kind = rec.get('kind', '')
    actual = rec.get('actual') or ''
    if kind.endswith('traceback'):
        # A traceback case whose exception line matches: the format differs.
        return 'traceback format' if last_exc(actual)[0] == last_exc(rec.get('expected') or '')[0] \
            else classify(*last_exc(actual), kind)
    if kind.endswith('unexpected exception'):
        return classify(*last_exc(actual), kind)
    if kind.endswith('result') and rec.get('expectation') == 'Raise':
        t = re.match(r'(\w+)\(', actual)
        want = re.match(r'(\w+)\(', rec.get('expected') or '')
        if t and want and t.group(1) == want.group(1):
            return 'exception message'
        return classify(t.group(1) if t else None, actual, kind)
    return 'semantics'


def last_exc(text: str) -> tuple[str | None, str]:
    for line in reversed(text.splitlines()):
        line = line.strip()
        m = EXC_LINE.match(line)
        if m and (m.group(1)[0].isupper()):
            return m.group(1), m.group(2) or ''
    return None, ''


# ---------------------------------------------------------------------------
# Own cases


def run_cli(engine: str, path: Path, cwd: str) -> tuple[str, str, int]:
    cmd = {'cpython': [PYTHON, '-X', 'utf8', str(path)], 'mm': [str(MM), 'run', str(path)],
           'micropython': [str(BIN['micropython']), str(path)], 'pocketpy': [str(BIN['pocketpy']), str(path)]}[engine]
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=60, cwd=cwd, env=engine_env(engine),
                           stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return '', 'timeout', -999
    norm = lambda b: b.decode('utf-8', 'replace').replace(str(path), '<file>')  # noqa: E731
    return norm(p.stdout), norm(p.stderr), p.returncode


def own(engine: str, out: Path) -> None:
    cases = sorted((ROOT / 'conformance/own').glob('*.py'))

    def one(path: Path) -> dict:
        with tempfile.TemporaryDirectory() as d:
            ref = run_cli('cpython', path, d)
            got = run_cli(engine, path, d)
        rec = {'file': path.name, 'status': 'pass' if got[0] == ref[0] and (got[2] == 0) == (ref[2] == 0) else 'fail',
               'strict': got == ref}
        if got[2] == -999:
            rec.update(status='timeout', cause='timeout')
        elif got[2] < 0 or got[2] > 128:
            rec.update(status='crash', cause='crash', signal=got[2])
        elif rec['status'] == 'fail':
            t, m = last_exc(got[1] if got[1].strip() else got[0])
            rec['cause'] = classify(t, m, '') if got[2] != 0 and ref[2] == 0 or (t and t != last_exc(ref[1])[0]) \
                else 'semantics'
            rec['error'] = f'{t}: {m}'[:200] if t else None
            rec['first_diff'] = first_diff(ref[0], got[0])
        return rec

    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        recs = list(pool.map(one, cases))
    with (out / f'own-{engine}.jsonl').open('w') as f:
        for r in recs:
            f.write(json.dumps(r) + '\n')


def first_diff(a: str, b: str) -> str:
    al, bl = a.splitlines(), b.splitlines()
    for i in range(max(len(al), len(bl))):
        x = al[i] if i < len(al) else '<none>'
        y = bl[i] if i < len(bl) else '<none>'
        if x != y:
            return f'line {i + 1}: expected {x[:80]!r}, got {y[:80]!r}'
    return ''


# ---------------------------------------------------------------------------
# Codemode


def codemode(engine: str, out: Path) -> None:
    sys.path.insert(0, str(ROOT / 'conformance/codemode'))
    import run as cm  # conformance/codemode/run.py: run_one, SETS, solutions, cause

    ref = {}
    for ln in (ROOT / 'conformance/codemode/results/runs.jsonl').read_text().splitlines():
        r = json.loads(ln)
        ref[(r['set'], r['task'])] = r
    base = engine.removesuffix('-sync')
    cmd = [str(BIN[base]), str(HERE / 'codemode' / f'{base}_shim.py')]
    items = cm.solutions(os.environ.get('FILTER') or None)
    env_engine = engine_env(engine)

    def one(item):
        set_name, task_id, path = item
        source = path.read_text()
        task = cm.ptc.TASKS[task_id]
        mode = cm.SETS[set_name]
        if engine == 'pocketpy-sync':
            source = sync_rewrite(source)
        elif engine == 'micropython':
            source = wrap_top_level_await(source)
        rec = cm.run_one(cmd, task, mode, source)
        rec_out = {'set': set_name, 'task': task_id, 'category': task.category,
                   'outcome': rec['outcome'], 'error': rec['error'], 'problem': rec['problem'],
                   'cause': cm.cause(rec), 'calls': len(rec['calls'])}
        r0 = ref.get((set_name, task_id))
        if r0:
            rec_out['cpython'] = r0['cpython']['outcome']
            rec_out['same_calls'] = [[c[0], c[1]] for c in rec['calls']] == \
                [[c[0], c[1]] for c in r0['cpython'].get('calls', [])] if 'calls' in r0['cpython'] else None
        return rec_out

    os.environ.update({k: v for k, v in env_engine.items() if k == 'MICROPYPATH'})
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        recs = list(pool.map(one, items))
    with (out / f'codemode-{engine}.jsonl').open('w') as f:
        for r in recs:
            f.write(json.dumps(r) + '\n')


def sync_rewrite(source: str) -> str:
    """Codemode source with its async layer removed, for an engine without
    `async`/`await` (PocketPy) whose host exposes tools as plain functions:
    `await x` -> `x`, `async def` -> `def`, `async for/with` -> `for/with`,
    `asyncio.run(c)` -> `c`, `asyncio.sleep(t)` -> `None`, `import asyncio` dropped, `asyncio.gather(*xs)` -> `[*xs]` (calls made in
    order). A harness transform, reported apart from the engine's own result."""
    import ast

    try:
        tree = ast.parse(source)
    except SyntaxError:
        return source

    class T(ast.NodeTransformer):
        def visit_Await(self, node):
            return self.visit(node.value)

        def visit_AsyncFunctionDef(self, node):
            node = self.generic_visit(node)
            return ast.copy_location(ast.FunctionDef(**{f: getattr(node, f) for f in node._fields}), node)

        def visit_AsyncFor(self, node):
            node = self.generic_visit(node)
            return ast.copy_location(ast.For(**{f: getattr(node, f) for f in node._fields}), node)

        def visit_AsyncWith(self, node):
            node = self.generic_visit(node)
            return ast.copy_location(ast.With(**{f: getattr(node, f) for f in node._fields}), node)

        def visit_Import(self, node):
            # `import asyncio` goes with the async layer it served.
            node.names = [a for a in node.names if a.name != 'asyncio']
            return node if node.names else ast.Pass()

        def visit_Call(self, node):
            node = self.generic_visit(node)
            f = node.func
            if isinstance(f, ast.Attribute) and isinstance(f.value, ast.Name) and f.value.id == 'asyncio':
                if f.attr == 'run' and node.args:
                    return node.args[0]
                if f.attr == 'gather':
                    return ast.List(elts=node.args, ctx=ast.Load())
                if f.attr == 'sleep':  # virtual, as on the other engines
                    return ast.Constant(None)
            return node

    return ast.unparse(ast.fix_missing_locations(T().visit(tree))) + '\n'


def wrap_top_level_await(source: str) -> str:
    """A solution with top-level `await` as an `async def` run by
    `asyncio.run`, for an engine that compiles no top-level await
    (MicroPython). Every name the module body binds is declared `global`, so
    the body keeps module semantics; lines that continue a multi-line string
    are not indented. Unchanged if the source has no top-level await."""
    import ast
    import io
    import tokenize

    try:
        flags = compile(source, 's', 'exec', flags=ast.PyCF_ALLOW_TOP_LEVEL_AWAIT, dont_inherit=True).co_flags
    except SyntaxError:
        return source
    if not flags & 0x80:  # CO_COROUTINE
        return source
    tree = ast.parse(source)
    names: set[str] = set()

    def bind(node, top=True):
        for child in ast.iter_child_nodes(node):
            if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                names.add(child.name)
                continue
            if isinstance(child, (ast.Lambda, ast.ListComp, ast.SetComp, ast.DictComp, ast.GeneratorExp)):
                continue
            if isinstance(child, ast.Name) and isinstance(child.ctx, ast.Store):
                names.add(child.id)
            elif isinstance(child, (ast.Import, ast.ImportFrom)):
                for a in child.names:
                    if a.name != '*':
                        names.add((a.asname or a.name).split('.')[0])
            elif isinstance(child, ast.ExceptHandler) and child.name:
                names.add(child.name)
            elif isinstance(child, ast.NamedExpr):
                names.add(child.target.id)
            bind(child)

    bind(tree)
    keep = set()  # rows (1-based) inside a multi-line string: not indented
    toks = tokenize.generate_tokens(io.StringIO(source).readline)
    open_fstring = []
    for tok in toks:
        if tok.type == tokenize.STRING and tok.end[0] > tok.start[0]:
            keep.update(range(tok.start[0] + 1, tok.end[0] + 1))
        elif tok.type == getattr(tokenize, 'FSTRING_START', -1):
            open_fstring.append(tok.start[0])
        elif tok.type == getattr(tokenize, 'FSTRING_END', -1) and open_fstring:
            start = open_fstring.pop()
            if tok.end[0] > start:
                keep.update(range(start + 1, tok.end[0] + 1))
    lines = source.splitlines()
    body = [ln if i + 1 in keep or not ln.strip() else '    ' + ln for i, ln in enumerate(lines)]
    head = ['async def __codemode_main():']
    if names:
        head.append('    global ' + ', '.join(sorted(names)))
    return '\n'.join(head + body + ['    pass', 'import asyncio as __codemode_asyncio',
                                    '__codemode_asyncio.run(__codemode_main())', ''])


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('what', choices=['corpus', 'own', 'codemode'])
    ap.add_argument('--engines', default='micropython,pocketpy,mm')
    ap.add_argument('--out', required=True)
    a = ap.parse_args()
    # Every engine process inherits a 4 GiB address-space cap: PocketPy has
    # no memory limit, and one own case alone grows past 13 GB.
    resource.setrlimit(resource.RLIMIT_AS, (4 << 30, 4 << 30))
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    for e in a.engines.split(','):
        print(f'{a.what} {e}', file=sys.stderr, flush=True)
        {'corpus': corpus, 'own': own, 'codemode': codemode}[a.what](e, out)
    return 0


if __name__ == '__main__':
    sys.exit(main())
