"""Like-for-like correctness comparison of Python engines against CPython 3.14.

Usage (run with CPython 3.14, isolated):

    python3.14 -I conformance/breadth/comparators/run.py [--jobs 4] [--only ENGINE,...]
        [--filter SUBSTRING] [--out DIR]
    python3.14 -I conformance/breadth/comparators/run.py --report [results.jsonl]

The second form prints the README's Markdown tables and cause lists.

Every case becomes one script file. CPython, micro-monty (`mm run`), Monty
(`monty FILE`) and qpy (`qpy -B FILE`) each run that same file in a fresh
process, in an empty scratch directory, with a 30 s timeout. The script's
observable result is its stdout and its final uncaught exception (type
without module prefix, plus message), as read from the traceback on stderr.
A case passes on an engine when that result equals CPython's.

Corpora:
- `own`: `conformance/own/*.py`, run unchanged.
- `monty`: `conformance/monty/*.py` whose primary tag (as in
  `conformance/results/cpython-3.14.jsonl`) is `cpython`. The case is parsed by
  `conformance/oracle.py` (imported read-only): its code without the
  expectation comment/TRACEBACK docstring; `Return=`/`Return.str=`/`Return.type=`
  cases print `repr`/`str`/`type(...).__name__` of the final expression line;
  `run-async` cases get the datatest's `asyncio.run` wrapper. The case's
  encoded expectation is not used for pass/fail (CPython's run of the same
  script is the reference); `expected_ok` records whether CPython's result
  agrees with it.
- `bench`: the 13 `bench/*.py` programs; stdout must equal `bench/expected.txt`.

Writes `results.jsonl` (one line per corpus/case/engine) into --out
(default: this directory).
"""

import ast
import concurrent.futures
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.realpath(__file__))
REPO = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
CONF = os.path.join(REPO, 'conformance')

PY = os.environ.get('PYTHON', '/home/user/.local/bin/python3.14')
MM = os.environ.get('MM', '/tmp/mm-evidence-layers/main/mm')
MONTY = os.environ.get('MONTY', '/tmp/mm-evidence-baselines/venv/bin/monty')
QPY = os.environ.get('QPY', '/tmp/mm-evidence-dquickts/py/qpy')
QPYHOME = os.environ.get('QPYHOME', os.path.dirname(QPY))
TIMEOUT = 30.0
ENGINES = ('cpython', 'micro-monty', 'monty', 'qpy')


def base_env(zone):
    env = {k: v for k, v in os.environ.items() if not k.startswith('PYTHON')}
    env['PYTHONHASHSEED'] = '0'
    env['PYTHONDONTWRITEBYTECODE'] = '1'
    env['TZ'] = zone or 'UTC'
    return env


def command(engine, path, zone):
    env = base_env(zone)
    if engine == 'cpython':
        # Not -I: `import__sys.py` asserts the default `sys.flags`.
        return [PY, '-B', path], env
    if engine == 'micro-monty':
        return [MM, 'run', path], env
    if engine == 'monty':
        return [MONTY, path], env
    if engine == 'qpy':
        env['QPYHOME'] = QPYHOME
        return [QPY, '-B', path], env
    raise ValueError(engine)


# -- Reading results --------------------------------------------------------

ADDRESS = re.compile(r'0x[0-9a-fA-F]{4,}')
EXC_LINE = re.compile(r'^([A-Za-z_][\w.]*)(?:: (.*))?$')
MONTY_PREFIX = re.compile(r'^\S+ \u276f ')


def normalise(text, path):
    text = text.replace(path, os.path.basename(path))
    return ADDRESS.sub('0x…', text)


def parse_exception(stderr, engine):
    """The final uncaught exception in a traceback printed on stderr, as (type, message)."""
    lines = stderr.splitlines()
    if engine == 'monty':
        # `<duration> ❯ error: Traceback ...` or `error:\nTraceback ...`
        lines = [MONTY_PREFIX.sub('', line) for line in lines]
        lines = [line[len('error:'):].lstrip() if line.startswith('error:') else line for line in lines]
    start = 0
    for index, line in enumerate(lines):
        if line.startswith('Traceback (most recent call last):'):
            start = index + 1
    for index in range(start, len(lines)):
        line = lines[index]
        if not line or line[0].isspace() or line.startswith(('Traceback', 'During handling', 'The above exception')):
            continue
        m = EXC_LINE.match(line)
        if m is None or (m.group(2) is None and not m.group(1)[:1].isupper()):
            continue
        message = '\n'.join([m.group(2) or ''] + lines[index + 1 :]).rstrip()
        return m.group(1).rpartition('.')[2], message
    return None


SUGGESTION = re.compile(r"\.? (Did you mean[: ].*\?|Or did you forget to import '\w+'\?|Did you forget to import '\w+'\?)$")


def key_message(exc_type, message):
    """The message as `conformance/oracle.py` compares it (args[0]), from its display.

    CPython's traceback printer appends "Did you mean ...?" suggestions to
    NameError/AttributeError/ImportError lines; they are not part of args[0],
    so they are dropped for every engine. `KeyError` shows `str(exc)` (the
    key's repr) on CPython but `args[0]` on Monty's CLI; the key is read back."""
    message = SUGGESTION.sub('', message)
    if exc_type == 'KeyError':
        try:
            value = ast.literal_eval(message)
        except (ValueError, SyntaxError, MemoryError, RecursionError):
            return message
        if isinstance(value, str):
            return value
    return message


def run_one(engine, path, zone):
    cmd, env = command(engine, path, zone)
    scratch = tempfile.mkdtemp(prefix='cmp-run-')
    try:
        try:
            proc = subprocess.run(cmd, cwd=scratch, env=env, stdin=subprocess.DEVNULL,
                                  capture_output=True, timeout=TIMEOUT)
        except subprocess.TimeoutExpired as expired:
            return {'status': 'timeout', 'stdout': normalise((expired.stdout or b'').decode('utf-8', 'replace'), path),
                    'exc': None, 'returncode': None, 'stderr_tail': ''}
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    stdout = normalise(proc.stdout.decode('utf-8', 'replace'), path)
    stderr = normalise(proc.stderr.decode('utf-8', 'replace'), path)
    tail = '\n'.join(stderr.splitlines()[-20:])
    result = {'status': 'ran', 'stdout': stdout, 'exc': None, 'returncode': proc.returncode, 'stderr_tail': tail}
    if proc.returncode == 0:
        return result
    if proc.returncode < 0 or 'panicked at' in stderr or 'Fatal Python error' in stderr \
            or 'has overflowed its stack' in stderr:
        result['status'] = 'crash'
        return result
    exc = parse_exception(stderr, engine)
    if exc is None:
        result['exc'] = ['exit', str(proc.returncode)]
    else:
        result['exc'] = [exc[0], key_message(*exc)]
    return result


# -- Categories ---------------------------------------------------------------

UNSUPPORTED = re.compile(
    r'micro-monty does not support|does not yet support|not (yet )?(supported|implemented)(?! between)|unsupported',
    re.IGNORECASE)


def categorise(ref, got):
    if got['status'] in ('timeout', 'crash'):
        return got['status']
    rexc, gexc = ref['exc'], got['exc']
    if gexc == rexc and got['stdout'] == ref['stdout']:
        return 'pass'
    if gexc is not None and gexc != rexc:
        if gexc[0] == 'SyntaxError' and (rexc is None or rexc[0] != 'SyntaxError'):
            return 'unsupported-syntax'
        if gexc[0] == 'exit':
            return 'crash'
        if gexc[0] == 'NotImplementedError' or UNSUPPORTED.search(gexc[1] or ''):
            if not (rexc and UNSUPPORTED.search(rexc[1] or '')):
                return 'unsupported-feature'
        if gexc[0] in ('ModuleNotFoundError', 'ImportError') and (rexc is None or rexc[0] != gexc[0]):
            return 'missing-module'
        if gexc[0] == 'NameError' and (rexc is None or rexc[0] != 'NameError'):
            return 'missing-name'
        if gexc[0] == 'AttributeError' and (rexc is None or rexc[0] != 'AttributeError'):
            return 'missing-attribute'
        return 'unexpected-exception' if rexc is None else 'wrong-exception'
    if gexc != rexc:  # CPython raised, the engine did not
        return 'no-exception'
    return 'wrong-stdout'


CAUSES = [
    (r'does not yet support class inheritance', 'no class inheritance/metaclasses'),
    (r'does not yet support yield', 'no yield (generators)'),
    (r"does not yet support the 'del' statement", 'no del statement'),
    (r'does not yet support method decorators', 'no classmethod/staticmethod/property'),
    (r'does not yet support class bodies containing', 'class bodies limited to methods and simple variables'),
    (r'does not yet support (.*)', 'parser does not support {0}'),
    (r'Wildcard imports', 'no wildcard import'),
    (r'micro-monty does not support (\w+)', 'no {0}()'),
    (r'json.loads hooks are not supported', 'no json.loads hooks'),
    (r"Standard library module '(\w+)' was not found", 'C module {0} absent (CONFIG_PYTHON_CAPI off)'),
    (r"No module named '([\w.]+)'", 'missing module {0}'),
    (r"cannot import name '(\w+)' from '([\w.]+)'", 'missing {1}.{0}'),
    (r"module '([\w.]+)' has no attribute '(\w+)'", 'missing {0}.{1}'),
    (r"^AttributeError: '([\w.]+)' object has no attribute '(\w+)'", 'missing {0}.{1}'),
    (r"^NameError: name '(\w+)' is not defined$", 'missing name {0}'),
    (r'unsupported hash algorithm', 'hashlib algorithms absent'),
    (r'Internal error in monty', 'internal error: exception with several args'),
    (r'got an unexpected keyword argument', 'missing keyword argument support'),
    (r'^SyntaxError: no binding for nonlocal', 'nonlocal binding rejected at compile time'),
    (r'^AssertionError', 'assertion failed (semantic difference)'),
]


def cause(category, exc):
    if category in ('pass', 'timeout', 'crash'):
        return category
    if exc is not None:
        text = f'{exc[0]}: {exc[1]}'
        for pattern, label in CAUSES:
            m = re.search(pattern, text)
            if m:
                return label.format(*m.groups())
    return {'wrong-stdout': 'different stdout', 'no-exception': 'no exception raised'}.get(
        category, f'{category}: {exc[0] if exc else ""}')


def raised_at(stderr):
    """The last traceback frame's location and source line."""
    lines = stderr.splitlines()
    for index in range(len(lines) - 1, -1, -1):
        m = re.search(r'File "([^"]+)", line (\d+)', lines[index])
        if m:
            source = lines[index + 1].strip() if index + 1 < len(lines) else ''
            return f'{os.path.basename(m.group(1))}:{m.group(2)}: {source[:200]}'
    return ''


def first_difference(want, got):
    a, b = want.splitlines(), got.splitlines()
    for number in range(max(len(a), len(b))):
        x = a[number] if number < len(a) else '<none>'
        y = b[number] if number < len(b) else '<none>'
        if x != y:
            return f'line {number + 1}: expected {x[:150]!r}, got {y[:150]!r}'
    return ''


# -- Cases --------------------------------------------------------------------


def load_oracle():
    saved = list(sys.path)
    spec = importlib.util.spec_from_file_location('mm_oracle', os.path.join(CONF, 'oracle.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    sys.path[:] = saved
    return module


def monty_script(oracle, content):
    code, expectation, header = oracle.parse_fixture(content)
    kind = expectation[0]
    need = kind in ('Return', 'ReturnStr', 'ReturnType')
    if header['run-async']:
        statements, expr = oracle.wrap_code_for_async(code, need)
    else:
        statements, expr = oracle.split_code_for_module(code, need)
    if expr is not None:
        show = {'Return': 'repr(({}))', 'ReturnStr': 'str(({}))', 'ReturnType': 'type(({})).__name__'}[kind]
        statements += '\nprint(' + show.format(expr) + ')\n'
    else:
        statements += '\n'
    return statements, expectation, header


def expectation_ok(expectation, ref):
    """Whether CPython's script run agrees with the case's encoded expectation."""
    kind, expected = expectation
    if ref['status'] != 'ran':
        return False
    exc = ref['exc']
    if kind in ('Return', 'ReturnStr', 'ReturnType'):
        return exc is None and ref['stdout'].rstrip('\n').splitlines()[-1:] == [expected]
    if kind == 'Raise':
        if exc is None:
            return False
        msg = exc[1]
        shown = f'{exc[0]}()' if not msg else (f'{exc[0]}("{msg}")' if "'" in msg else f"{exc[0]}('{msg}')")
        return shown == expected
    if kind == 'Traceback':
        last = expected.strip().splitlines()[-1] if expected.strip() else ''
        return exc is not None and last.split(':')[0].rpartition('.')[2] == exc[0]
    return exc is None  # NoException


def collect(work, flt):
    oracle = load_oracle()
    tags = {}
    with open(os.path.join(CONF, 'results', 'cpython-3.14.jsonl'), encoding='utf-8') as f:
        for line in f:
            row = json.loads(line)
            tags[row['file']] = row['tags']
    cases = []
    for name in sorted(os.listdir(os.path.join(CONF, 'own'))):
        if name.endswith('.py'):
            src = os.path.join(CONF, 'own', name)
            dst = os.path.join(work, 'own', name)
            shutil.copyfile(src, dst)
            cases.append({'corpus': 'own', 'case': name, 'path': dst, 'zone': None})
    for name in sorted(os.listdir(os.path.join(CONF, 'monty'))):
        if not name.endswith('.py') or tags.get(name, [''])[0] != 'cpython':
            continue
        with open(os.path.join(CONF, 'monty', name), encoding='utf-8', newline='') as f:
            content = f.read()
        script, expectation, header = monty_script(oracle, content)
        dst = os.path.join(work, 'monty', name)
        with open(dst, 'w', encoding='utf-8', newline='') as f:
            f.write(script)
        cases.append({'corpus': 'monty', 'case': name, 'path': dst, 'zone': header['timezone'],
                      'expectation': list(expectation), 'async': header['run-async']})
    expected = {}
    with open(os.path.join(REPO, 'bench', 'expected.txt'), encoding='utf-8') as f:
        for line in f:
            if line.strip() and not line.startswith('#'):
                name, _, out = line.rstrip('\n').partition(': ')
                expected[name] = out
    for name, out in sorted(expected.items()):
        file = 'json_rows.py' if name == 'json' else f'{name}.py'
        dst = os.path.join(work, 'bench', file)
        shutil.copyfile(os.path.join(REPO, 'bench', file), dst)
        cases.append({'corpus': 'bench', 'case': file, 'path': dst, 'zone': None, 'bench_expected': out + '\n'})
    if flt:
        cases = [c for c in cases if flt in c['case']]
    return cases


def report(path):
    """Markdown tables from results.jsonl: per corpus, pass and categories per
    engine; then each engine's failures grouped by cause."""
    rows = [json.loads(line) for line in open(path, encoding='utf-8')]
    engines = [e for e in ENGINES if any(r['engine'] == e for r in rows)]
    out = []
    for corpus in ('own', 'monty', 'bench'):
        sub = [r for r in rows if r['corpus'] == corpus]
        n = len({r['case'] for r in sub})
        out.append(f'### `{corpus}` ({n} cases)\n')
        out.append('| Engine | Pass | % | Failure categories |')
        out.append('|---|---:|---:|---|')
        for e in engines:
            mine = [r for r in sub if r['engine'] == e]
            ok = sum(r['category'] == 'pass' for r in mine)
            cats = {}
            for r in mine:
                if r['category'] != 'pass':
                    cats[r['category']] = cats.get(r['category'], 0) + 1
            shown = ', '.join(f'{k} {v}' for k, v in sorted(cats.items(), key=lambda kv: (-kv[1], kv[0]))) or '-'
            out.append(f'| {e} | {ok}/{len(mine)} | {100 * ok / len(mine):.1f} | {shown} |')
        out.append('')
    for e in engines:
        if e == 'cpython':
            continue
        fails = [r for r in rows if r['engine'] == e and r['category'] != 'pass']
        out.append(f'### {e}: {len(fails)} failures by cause\n')
        groups = {}
        for r in fails:
            groups.setdefault(r['cause'], []).append(r)
        for label, items in sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[0])):
            names = ', '.join(f"`{r['case'][:-3]}`" + ('' if r['corpus'] == 'monty' else f" ({r['corpus']})")
                              for r in items)
            out.append(f'- **{label}** ({len(items)}): {names}')
        out.append('')
    print('\n'.join(out))


def main(argv):
    jobs, only, flt, out = 4, list(ENGINES), None, HERE
    args = argv[1:]
    if args[:1] == ['--report']:
        return report(args[1] if len(args) > 1 else os.path.join(HERE, 'results.jsonl'))
    while args:
        opt, val, args = args[0], args[1], args[2:]
        if opt == '--jobs':
            jobs = int(val)
        elif opt == '--only':
            only = val.split(',')
        elif opt == '--filter':
            flt = val
        elif opt == '--out':
            out = val
        else:
            sys.exit(f'unknown option {opt}')
    if 'cpython' not in only:
        only.insert(0, 'cpython')
    work = tempfile.mkdtemp(prefix='cmp-cases-')
    for corpus in ('own', 'monty', 'bench'):
        os.makedirs(os.path.join(work, corpus))
    cases = collect(work, flt)
    tasks = [(c, e) for c in cases for e in only]
    results = {}
    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        futures = {pool.submit(run_one, e, c['path'], c['zone']): (c['corpus'], c['case'], e) for c, e in tasks}
        for done, future in enumerate(concurrent.futures.as_completed(futures), 1):
            results[futures[future]] = future.result()
            if done % 200 == 0:
                print(f'{done}/{len(tasks)}', file=sys.stderr, flush=True)
    rows = []
    for c in cases:
        ref = results[(c['corpus'], c['case'], 'cpython')]
        for e in only:
            got = results[(c['corpus'], c['case'], e)]
            if c['corpus'] == 'bench':
                ok = got['status'] == 'ran' and got['exc'] is None and got['stdout'] == c['bench_expected']
                category = 'pass' if ok else ('wrong-stdout' if got['status'] == 'ran' and got['exc'] is None
                                              else categorise({'exc': None, 'stdout': c['bench_expected'],
                                                               'status': 'ran'}, got))
            else:
                category = 'pass' if e == 'cpython' else categorise(ref, got)
            want = c['bench_expected'] if c['corpus'] == 'bench' else ref['stdout']
            row = {'corpus': c['corpus'], 'case': c['case'], 'engine': e, 'category': category,
                   'cause': cause(category, got['exc']), 'status': got['status'],
                   'returncode': got['returncode'], 'exc': got['exc'],
                   'ref_exc': None if c['corpus'] == 'bench' else ref['exc'],
                   'stdout_diff': first_difference(want, got['stdout']) if category != 'pass' else '',
                   'raised_at': raised_at(got['stderr_tail']) if got['exc'] else '',
                   # Output is kept for non-passes only (CPython's is the reference).
                   'stdout': '' if category == 'pass' else got['stdout'][-1000:],
                   'stderr_tail': '' if category == 'pass' else got['stderr_tail'][-1500:]}
            if c['corpus'] == 'monty':
                row['expectation'] = c['expectation'][0]
                if e == 'cpython':
                    row['expected_ok'] = expectation_ok(c['expectation'], ref)
            rows.append(row)
    with open(os.path.join(out, 'results.jsonl'), 'w', encoding='utf-8') as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False) + '\n')
    shutil.rmtree(work, ignore_errors=True)
    summary = {}
    for row in rows:
        s = summary.setdefault((row['corpus'], row['engine']), {})
        s[row['category']] = s.get(row['category'], 0) + 1
    for (corpus, engine), s in sorted(summary.items()):
        print(corpus, engine, sum(s.values()), json.dumps(dict(sorted(s.items(), key=lambda kv: -kv[1]))))


if __name__ == '__main__':
    main(sys.argv)
