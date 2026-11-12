#!/home/user/.local/bin/python3.14 -I
"""`mm exec --json` for MicroPython and PocketPy, so conformance/oracle.py's
micro-monty side judges them unchanged (`oracle.py --engine micro-monty --mm
adapters/pyexec.py`).

    pyexec.py micropython|pocketpy exec --json [--recursion-limit N]
        [--host-fixtures] [--main-module] FILE

It reads FILE in the current directory and runs it in the engine's CLI
through a small driver written in the subset both engines run: the case is
`exec`uted in a fresh globals dict, then its last expression statement, if
any, is `eval`uated there, as `mm exec` reports it. Top-level `await` is
wrapped in `async def` + `asyncio.run`, as the oracle wraps `run-async`
cases. The driver prints one marker line with the report:
  {"error": {"type", "message", "traceback"} | null,
   "result": {"repr", "str", "type"} | null}
The split into statements and the last expression is the only thing done on
CPython's side (its `ast`), on source text; nothing else is translated.
`--recursion-limit` and `--host-fixtures` are accepted and ignored: neither
engine has the datatest's host fixtures, so `run.py` skips `call-external`
cases for them.

The engine is the first argument; `pyexec-micropython` and `pyexec-pocketpy`
pass it (the oracle passes the runner only part of the environment once a
traceback case has run, so it cannot come from there). The binaries are
`$PENG/bin/micropython` (the unix port; `MICROPYPATH` is set to its frozen
modules and `$PENG/mpylib`) and `$PENG/bin/pocketpy`; `PENG` defaults to
`/tmp/peng` (README "Builds").
"""

import ast
import json
import os
import signal
import subprocess
import sys

MARK = '@@REPORT@@'

DRIVER = r'''
import json
import sys
__mm_f = open(__MM_SRC__, 'r')
__mm_src = __mm_f.read()
__mm_f.close()
__mm_expr = __MM_EXPR__
__mm_g = __MM_GLOBALS__
__mm_rep = {'error': None, 'result': None}


def __mm_tb(e):
__MM_TB__


try:
    exec(compile(__mm_src, __MM_NAME__, 'exec'), __mm_g)
    if __mm_expr is not None:
        __mm_r = eval(__mm_expr, __mm_g)
        __mm_rep['result'] = {'repr': repr(__mm_r), 'str': str(__mm_r), 'type': type(__mm_r).__name__}
except BaseException as __mm_e:
    __mm_t = __mm_tb(__mm_e)
    __mm_last = type(__mm_e).__name__ + ': ' + str(__mm_e)
    if not __mm_t.strip().endswith(__mm_last.strip()):
        # The engine's traceback text lacks the exception line (PocketPy's
        # `format_exc` gives none after `exec`): add it, as CPython ends one.
        __mm_t = __mm_t + __mm_last + '\n'
    __mm_rep['error'] = {'type': type(__mm_e).__name__, 'message': str(__mm_e), 'traceback': __mm_t}
print('__MM_MARK__' + json.dumps(__mm_rep))
'''

TB = {
    'micropython': '''    try:
        import io
        buf = io.StringIO()
        sys.print_exception(e, buf)
        return buf.getvalue()
    except BaseException:
        return ''
''',
    'pocketpy': '''    try:
        import traceback
        t = traceback.format_exc()
        return t if t is not None else ''
    except BaseException:
        return ''
''',
}


def split(code):
    """(statements, last expression or None), and whether it awaits at top level."""
    try:
        tree = ast.parse(code)
    except SyntaxError:
        return code, None, False
    try:
        flags = compile(code, 'case', 'exec', flags=ast.PyCF_ALLOW_TOP_LEVEL_AWAIT, dont_inherit=True).co_flags
        is_async = bool(flags & 0x80)  # CO_COROUTINE
    except SyntaxError:
        is_async = False
    lines = code.splitlines(keepends=True)
    last = tree.body[-1] if tree.body else None
    if isinstance(last, ast.Expr):
        stmts = ''.join(lines[: last.lineno - 1])
        expr = ast.get_source_segment(code, last)
        # A last expression sharing its first line with a previous statement
        # (`a = 1; a`) stays in the statements; nothing is evaluated.
        if lines[last.lineno - 1][: last.col_offset].strip():
            return code, None, is_async
        return stmts, expr, is_async
    return code, None, is_async


def wrap_async(stmts, expr):
    if stmts and not stmts.endswith('\n'):
        stmts += '\n'
    body = stmts + (f'return {expr}\n' if expr else 'pass\n')
    indented = ''.join('' if not ln.strip() else '    ' + ln for ln in body.splitlines(keepends=True))
    if not indented.endswith('\n'):
        indented += '\n'
    return f'import asyncio\nasync def __mm_main():\n{indented}__mm_result = asyncio.run(__mm_main())\n', '__mm_result'


def main(argv):
    engine, args = argv[1], argv[2:]
    if args[:2] != ['exec', '--json']:
        print('usage: pyexec.py exec --json [...] FILE', file=sys.stderr)
        return 2
    args = args[2:]
    main_module = False
    while args and args[0].startswith('--'):
        opt = args.pop(0)
        if opt == '--recursion-limit':
            args.pop(0)
        elif opt == '--main-module':
            main_module = True
    name = args[0]
    with open(name, encoding='utf-8', newline='') as f:
        code = f.read()
    stmts, expr, is_async = split(code)
    if is_async:
        stmts, expr = wrap_async(stmts, expr)
    with open('__mm_case.txt', 'w', encoding='utf-8', newline='') as f:
        f.write(stmts)
    globals_ = "{'__name__': '__main__', '__file__': %r}" % name if main_module else '{}'
    driver = (
        DRIVER.replace('__MM_SRC__', repr('__mm_case.txt'))
        .replace('__MM_EXPR__', repr(expr))
        .replace('__MM_GLOBALS__', globals_)
        .replace('__MM_NAME__', repr(name))
        .replace('__MM_MARK__', MARK)
        .replace('__MM_TB__', TB[engine])
    )
    with open('__mm_driver.py', 'w', encoding='utf-8') as f:
        f.write(driver)
    peng = os.environ.get('PENG', '/tmp/peng')
    env = dict(os.environ, MICROPYPATH=f'.frozen:{peng}/mpylib')
    cmd = [f'{peng}/bin/{engine}', '__mm_driver.py']
    proc = subprocess.run(cmd, stdin=subprocess.DEVNULL, capture_output=True, env=env)
    out = proc.stdout.decode('utf-8', 'replace')
    report = None
    for line in out.splitlines():
        if line.startswith(MARK):
            report = json.loads(line[len(MARK):])
            stdout = out[: out.index(MARK)]
    if report is None:
        err = proc.stderr.decode('utf-8', 'replace')
        if proc.returncode < 0:
            sys.stderr.write(f'{engine} killed by {signal.Signals(-proc.returncode).name}\n{err[-300:]}')
        else:
            sys.stderr.write(f'{engine} exited with status {proc.returncode}: {(out + err)[-300:]}')
        return 1
    report['stdout'] = stdout
    report['exit'] = 1 if report['error'] else 0
    print(json.dumps(report))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
