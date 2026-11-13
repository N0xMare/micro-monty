"""CPython oracle for micro-monty's conformance runner (`cargo xtask conformance`).

Runs Monty test cases (`conformance/monty/*.py`) the way the CPython side of
Monty's datatest harness runs them (pydantic/monty `66fff6bf`,
`crates/monty-datatest/src/main.rs`: `parse_fixture`, `try_run_cpython_test`,
`run_traceback_script`, `format_cpython_exception`, `split_code_for_module`,
`wrap_code_for_async`), and checks each case's expectation exactly as it does.
The CPython-side helpers the datatest imports (`scripts/test_fixtures.py`,
`scripts/run_traceback.py`, `scripts/cpython_watchdog.py`) are vendored
unmodified in `conformance/monty-scripts/`.

Usage: python3.14 conformance/oracle.py CASE.py [CASE.py ...]
       python3.14 conformance/oracle.py --engine micro-monty [--mm MM | --wasm MODULE --wasmtime WASMTIME]
           [--timeout S] CASE.py [...]
       python3.14 conformance/oracle.py --engine micro-monty --durability N[:M] [--mm MM] [--timeout S]
           [--alternate MODULE --wasmtime WASMTIME [--alternate-first native|wasm]] CASE.py [...]

The second form runs each case through micro-monty's `mm exec --json` (`--mm`,
else `$MM`) instead of CPython and checks the same expectations against mm's
report, as the Monty side of the datatest does; see "The micro-monty side"
below and `conformance/README.md` for its normalisations. With `--wasm
MODULE` it runs mm built for wasm32-wasip1 (a `.wasm` or a precompiled
`.cwasm`) under wasmtime instead, as the cross-target chain below does.

The third form is the durability test (docs/spec.md §13, `cargo xtask
durability`): each case runs as mm runs it in the second form (a case outside
`conformance/monty/` runs as a script, as `mm run` runs it: only with
`--main-module`), with the clock pinned and counter entropy (`mm --digest`),
in four modes: unparked; parked every N fuel units in one process ("parks",
`mm exec --park-every N`); the same with a snapshot at every stop, the
session going on ("snapshot", `--snapshot-continue`); and parked every N
fuel units and at every host call, each park saved to a snapshot by one `mm`
process and continued by a new one ("restore", `mm exec --snapshot-every N`,
then `mm resume` until done). With `N:M`, a case whose unparked run uses
more than N * M fuel parks every ceil(fuel / M) units instead, so no case
parks much more than M times. Each parked mode's report (stdout, result,
exception), fuel (every `mm` prints the fuel spent, counted exactly) and
final-state digest (the checksum of the session's image at the end) are
compared with the unparked run's (docs/spec.md §5.1, clause 4), and the
"restore" chain's with "snapshot"'s ("restore-vs-snapshot": a restored
session must continue as the session that took the same snapshots). With
`--alternate MODULE` (mm built for wasm32-wasip1, a `.wasm` or a
wasmtime-precompiled `.cwasm`), the parked chain alternates between the
native mm and MODULE under wasmtime, starting with `--alternate-first`
(default native), so every snapshot is restored on the other target
("cross"), and with `wasm` first the unparked run and the two in-process
modes also run on MODULE ("wasm", "wasm-parks", "wasm-snapshot"); the
reference run stays native. It prints
  {"file", "status": "pass"|"differs"|"crash"|"timeout"|"skip", "parks", "every",
   "diffs": {MODE: ["report"|"fuel"|"digest", ...]}, "fuel": {MODE: parked - unparked}, "reason"}

Prints one JSON object per case on stdout:
  {"file", "status": "pass"|"fail"|"error"|"skip", "kind", "reason",
   "expected", "actual", "expectation", "header": {...}}
micro-monty adds the statuses "crash" and "timeout" and, for a non-pass, a
first-cause "category".
`kind` is the datatest's failure kind (e.g. "CPython result"); `reason` is a
short, path- and address-free summary. With several cases, each runs in a
forked child so cases cannot leak state into each other.

Differences from the datatest that are deliberate, because each case runs in a
fresh process instead of one shared embedded interpreter:
- no `CPYTHON_TEST_LOCK`, `RecursionLimitGuard` or `forget_mount_imports`: there
  is nothing to serialise or restore;
- the 30 s watchdog is xtask's per-case timeout;
- the case's stdout goes to /dev/null (libtest does not show it either).
User code still runs with no Python frames below it, on a non-main thread, as
when pyo3 calls in from a libtest thread, so recursion-limit cases see the same
depth budget.
"""

import builtins
import collections
import itertools
import json
import os
import re
import shutil
import sys
import tempfile
import time
import tomllib
import types
import _thread

HERE = os.path.dirname(os.path.realpath(__file__))
SCRIPTS_DIR = os.path.join(HERE, 'monty-scripts')
REPO_ROOT = os.path.dirname(HERE)

# `TEST_RECURSION_LIMIT` in the datatest; `run_traceback.py` adds 5 for runpy.
TEST_RECURSION_LIMIT = 50

# The datatest's script directory comes first on `sys.path`, and an embedded
# interpreter has no script directory of its own, so this one replaces ours.
sys.path[0] = SCRIPTS_DIR


def import_script(name):
    """Imports a vendored script without writing `__pycache__` into the tree.

    Cases themselves run with bytecode writing as the interpreter started, as
    in the datatest (a `mount-fs` case can see `__pycache__` in its `root`).
    """
    previous = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        return __import__(name)
    finally:
        sys.dont_write_bytecode = previous

# -- `# /// monty` header (crates/monty-fixture/src/lib.rs) -------------------

BLOCK_START = '# /// monty'
BLOCK_END = '# ///'

BOOL_KEYS = ('call-external', 'run-async', 'mount-fs', 'skip-cpython-windows', 'cpython-main-module')
BENCH_KEYS = {'data': str, 'mount': bool, 'overlay': bool, 'limits': bool}


class HeaderError(Exception):
    """A malformed `# /// monty` header; the datatest panics on these."""


def rust_lines(text):
    """`str::lines()`: split on `\\n`, drop one trailing `\\r` per line and a final empty line."""
    if not text:
        return []
    parts = text.split('\n')
    if parts[-1] == '':
        parts.pop()
    return [part[:-1] if part.endswith('\r') else part for part in parts]


def is_block_marker(line):
    return line.startswith('#') and line.lstrip('#').lstrip().startswith('///')


def extract_block(source):
    block = None
    before_code = True
    lines = iter(enumerate(rust_lines(source)))
    for index, line in lines:
        before_code = before_code and (line.startswith('#') or line.strip() == '')
        if not is_block_marker(line):
            continue
        if line != BLOCK_START:
            problem = f'`{line}` is not `{BLOCK_START}` or a block\'s closing `{BLOCK_END}`'
        elif not before_code:
            problem = f'`{BLOCK_START}` must come before any code'
        elif block is not None:
            problem = f'second `{BLOCK_START}` block'
        else:
            block = (index, read_block_body(lines, index))
            continue
        raise HeaderError(f'line {index + 1}: {problem}')
    return block


def read_block_body(lines, start):
    toml = []
    for index, line in lines:
        if line == BLOCK_END:
            return ''.join(toml)
        if line == '#':
            content = ''
        elif line.startswith('# '):
            content = line[2:]
        else:
            raise HeaderError(f'line {index + 1}: `{line}` inside the header is not a `# ` comment')
        toml.append(content + '\n')
    raise HeaderError(f'line {start + 1}: `{BLOCK_START}` is never closed')


def parse_header(source):
    """`Header::parse`: strict, unknown keys and wrong types are errors."""
    header = {key: False for key in BOOL_KEYS}
    header.update({'xfail': [], 'gc-interval': None, 'timezone': None, 'bench': {}})
    block = extract_block(source)
    if block is None:
        return header
    start, text = block
    try:
        table = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        raise HeaderError(f'in the header opening on line {start + 1}: {error}') from None
    for key, value in table.items():
        if key in BOOL_KEYS:
            ok = isinstance(value, bool)
        elif key == 'xfail':
            ok = isinstance(value, list) and all(item in ('monty', 'cpython') for item in value)
        elif key == 'gc-interval':
            ok = isinstance(value, int) and not isinstance(value, bool) and value >= 0
        elif key == 'timezone':
            ok = isinstance(value, str)
        elif key == 'bench':
            ok = isinstance(value, dict) and all(
                name in BENCH_KEYS and isinstance(item, BENCH_KEYS[name]) for name, item in value.items()
            )
        else:
            raise HeaderError(f'in the header opening on line {start + 1}: unknown field `{key}`')
        if not ok:
            raise HeaderError(f'in the header opening on line {start + 1}: invalid value for `{key}`: {value!r}')
        header[key] = value
    return header


# -- Expectations (`parse_fixture`, `parse_traceback_expectation`) -------------

TRACEBACK_MARKER = '"""\nTRACEBACK:\n'


def parse_traceback_expectation(content):
    content = content.replace('\r\n', '\n')
    marker_pos = content.find(TRACEBACK_MARKER)
    if marker_pos < 0:
        return None
    code = '\n'.join(rust_lines(content[:marker_pos])).rstrip()
    after = content[marker_pos + len(TRACEBACK_MARKER) :]
    end = after.find('\n"""')
    if end < 0:
        return None
    return code, after[:end]


def parse_fixture(content):
    """Returns (code, (kind, expected), header). Order of checks as in the datatest."""
    lines = rust_lines(content)
    if not lines:
        raise HeaderError('Empty fixture file')
    header = parse_header(content)
    traceback_expectation = parse_traceback_expectation(content)
    if traceback_expectation is not None:
        code, expected = traceback_expectation
        return code, ('Traceback', expected), header
    last = lines[-1]
    for prefix, kind in (
        ('# ref-counts=', 'RefCounts'),
        ('# Return.str=', 'ReturnStr'),
        ('# Return.type=', 'ReturnType'),
        ('# Return=', 'Return'),
        ('# Raise=', 'Raise'),
    ):
        if last.startswith(prefix):
            return '\n'.join(lines[:-1]), (kind, last[len(prefix) :]), header
    return '\n'.join(lines), ('NoException', ''), header


def split_code_for_module(code, need_return_value):
    lines = rust_lines(code)
    nonempty = [i for i, line in enumerate(lines) if line.strip()]
    if not nonempty:
        raise HeaderError('Empty code')
    last_idx = nonempty[-1]
    if need_return_value:
        last_line = lines[last_idx].strip()
        if last_line.startswith('assert ') or last_line.startswith('assert('):
            return '\n'.join(lines[: last_idx + 1]), None
        return '\n'.join(lines[:last_idx]), last_line
    return '\n'.join(lines[: last_idx + 1]), None


def wrap_code_for_async(code, need_return_value):
    lines = rust_lines(code)
    candidates = [i for i, line in enumerate(lines) if line.strip() and not line.strip().startswith('#')]
    if not candidates:
        raise HeaderError('Empty code')
    last_idx = candidates[-1]
    indented = '\n'.join('' if line == '' else f'    {line}' for line in lines)
    return_stmt = f'\n    return {lines[last_idx].strip()}' if need_return_value else ''
    wrapped = (
        f'import asyncio\nasync def __test_main():\n{indented}{return_stmt}\n'
        '__test_result__ = asyncio.run(__test_main())'
    )
    return wrapped, ('__test_result__' if need_return_value else None)


# -- Running code the way pyo3 does -------------------------------------------


# The interpreter's default limit, which the oracle's own code runs under. The
# case's limit (changed by `sys.setrecursionlimit` in the case, or by
# `run_traceback.py`) is carried from one call into the case to the next, as in
# the datatest's shared interpreter, and reset for every case.
ORACLE_RECURSION_LIMIT = sys.getrecursionlimit()
case_recursion_limit = [ORACLE_RECURSION_LIMIT]


def in_fresh_thread(call):
    """Evaluates `call`, a C iterator yielding one value, on a new thread.

    pyo3 calls `py.run`/`py.eval`/`run_file_and_get_traceback` straight from
    Rust on a libtest worker thread, so the callee is the first Python frame:
    after `sys.setrecursionlimit(N)` a case has N frames to itself. Driving the
    call through `deque(map(...))` keeps every frame below it in C, and this
    thread waits using C calls only, so a low limit set by the case cannot trip
    it. An escaping exception reaches `sys.unraisablehook` as "in thread
    started by"; `_thread` swallows SystemExit, reported as a bare
    `SystemExit()`.

    Returns ('ok', value) or ('raised', exception).
    """
    results = []
    raised = []
    started = _thread.allocate_lock()
    started.acquire()
    previous_hook = sys.unraisablehook

    def hook(unraisable):
        # 3.12 passes the thread's callable as `object`; 3.14 names it in
        # `err_msg` and passes None.
        message = unraisable.err_msg or ''
        if 'thread started by' in message and (
            unraisable.object is collections.deque or 'collections.deque' in message
        ):
            raised.append(unraisable.exc_value)
        else:
            previous_hook(unraisable)

    sys.unraisablehook = hook
    base = _thread._count()
    try:
        steps = itertools.chain(
            map(sys.setrecursionlimit, [case_recursion_limit[0]]),
            map(_thread.LockType.release, [started]),  # the thread is counted by now
            map(results.append, call),
        )
        _thread.start_new_thread(collections.deque, (steps, 0))
        del steps
        started.acquire()
        while _thread._count() > base:
            time.sleep(0.0005)
    finally:
        case_recursion_limit[0] = sys.getrecursionlimit()
        sys.setrecursionlimit(ORACLE_RECURSION_LIMIT)
        sys.unraisablehook = previous_hook
    if raised:
        return 'raised', raised[0]
    if results:
        return 'ok', results[0]
    return 'raised', SystemExit()


def call_like_pyo3(func, *args):
    return in_fresh_thread(map(func, *[[arg] for arg in args]))


def run_like_pyo3(source, mode, globals_):
    """`Python::run` (mode 'exec') or `Python::eval` (mode 'eval'): compile as
    `<string>` without inheriting compiler flags, evaluate in `globals_`."""
    runner = exec if mode == 'exec' else eval
    return in_fresh_thread(map(runner, map(compile, [source], ['<string>'], [mode], [0], [True]), [globals_]))


def format_traceback(exc):
    """`format_traceback`: `run_traceback.format_full_traceback` of the exception."""
    run_traceback = import_script('run_traceback')
    status, value = call_like_pyo3(run_traceback.format_full_traceback, exc)
    if status == 'raised':
        raise Failure('Panic', 'no panic', f'Failed to call format_full_traceback: {value!r}')
    return value


def format_cpython_exception(exc):
    """`format_cpython_exception`: `Type('args[0]')` when args[0] is a str, else `Type()`."""
    exc_type = type(exc).__name__
    try:
        message = exc.args[0]
    except Exception:
        message = ''
    if not isinstance(message, str):
        message = ''
    try:
        message.encode('utf-8')
    except UnicodeEncodeError:
        message = ''  # pyo3 cannot extract it as a Rust String
    if not message:
        return f'{exc_type}()'
    if "'" in message:
        return f'{exc_type}("{message}")'
    return f"{exc_type}('{message}')"


def apply_timezone(zone):
    """`CpythonTimeZone::apply`: TZ is the header's zone, or UTC, via putenv + tzset."""
    os.putenv('TZ', zone or 'UTC')
    time.tzset()


def create_mount_fs_tempdir():
    """`create_mount_fs_tempdir`: the fixture tree `mount-fs` cases see as `root`."""
    root = tempfile.mkdtemp()
    files = {
        'hello.txt': b'hello world\n',
        'empty.txt': b'',
        'data.bin': b'\x00\x01\x02\x03',
        'subdir/nested.txt': b'nested content',
        'subdir/deep/file.txt': b'deep file',
        'readonly.txt': b'readonly content',
    }
    os.makedirs(os.path.join(root, 'subdir', 'deep'))
    for name, data in files.items():
        with open(os.path.join(root, name), 'wb') as f:
            f.write(data)
    try:
        os.symlink('missing', os.path.join(root, 'subdir', 'deep', 'dangling'))
    except OSError:
        pass
    return root


class Failure(Exception):
    def __init__(self, kind, expected, actual):
        super().__init__(kind)
        self.kind = kind
        self.expected = expected
        self.actual = actual


def try_run_cpython_test(abs_path, code, expectation, header):
    """`try_run_cpython_test`: returns None on success, raises Failure otherwise."""
    kind, expected = expectation
    iter_mode = header['call-external'] or header['mount-fs']
    async_mode = header['run-async']
    apply_timezone(header['timezone'])

    if kind == 'Traceback':
        run_traceback = import_script('run_traceback')
        status, value = call_like_pyo3(
            run_traceback.run_file_and_get_traceback, abs_path, TEST_RECURSION_LIMIT, iter_mode, async_mode
        )
        if status == 'raised':
            result = format_traceback(value)
        else:
            result = '' if value is None else value
        if result != expected:
            raise Failure('CPython traceback', expected, result)
        return

    mount_root = create_mount_fs_tempdir() if header['mount-fs'] else None
    try:
        run_statements(abs_path, code, expectation, header, async_mode, mount_root)
    finally:
        if mount_root is not None:
            shutil.rmtree(mount_root, ignore_errors=True)


def run_statements(abs_path, code, expectation, header, async_mode, mount_root):
    test_fixtures = import_script('test_fixtures')
    kind, expected = expectation
    need_return_value = kind in ('Return', 'ReturnStr', 'ReturnType')
    if async_mode:
        statements, expr = wrap_code_for_async(code, need_return_value)
    else:
        statements, expr = split_code_for_module(code, need_return_value)
    if '\0' in statements or (expr is not None and '\0' in expr):
        raise Failure('Panic', 'no panic', 'Invalid C string (NUL byte) in the case code')

    globals_ = {}
    globals_.update(test_fixtures.exported_globals)
    if header['cpython-main-module']:
        globals_['__name__'] = '__main__'
        globals_['__file__'] = abs_path
    if mount_root is not None:
        escaped = mount_root.replace('\\', '\\\\').replace("'", "\\'")
        setup = (
            'from pathlib import Path as _Path; import sys as _sys; '
            f"root = _Path('{escaped}'); _sys.path.insert(0, str(root))"
        )
        status, value = run_like_pyo3(setup, 'exec', globals_)
        if status == 'raised':
            raise Failure('Panic', 'no panic', f'Failed to set up mount-fs root for CPython: {value!r}')

    status, value = run_like_pyo3(statements, 'exec', globals_)
    if status == 'raised':
        actual = unexpected_or_raised(kind, value)
    elif expr is not None:
        status, value = run_like_pyo3(expr, 'eval', globals_)
        if status == 'raised':
            actual = unexpected_or_raised(kind, value)
        elif kind == 'Return':
            actual = render(repr, value)
        elif kind == 'ReturnStr':
            actual = render(str, value)
        elif kind == 'ReturnType':
            actual = type(value).__name__
        else:  # Raise
            raise Failure('CPython exception', expected, 'no exception raised')
    elif kind == 'Raise':
        raise Failure('CPython exception', expected, 'no exception raised')
    else:
        return  # NoException
    if actual != expected:
        raise Failure('CPython result', expected, actual)


def unexpected_or_raised(kind, exc):
    if kind == 'NoException':
        raise Failure('CPython unexpected exception', 'no exception', format_traceback(exc))
    if kind == 'Raise':
        return format_cpython_exception(exc)
    raise Failure('CPython unexpected exception', 'success', format_traceback(exc))


def render(func, value):
    """`result.repr().unwrap()` / `result.str().unwrap()`: the datatest panics if these raise."""
    status, rendered = call_like_pyo3(func, value)
    if status == 'raised':
        raise Failure('Panic', 'no panic', f'{func.__name__}() of the result raised {rendered!r}')
    return rendered


# -- Reporting ----------------------------------------------------------------

ADDRESS = re.compile(r'0x[0-9a-fA-F]{4,}')
PATCH_VERSION = '.'.join(map(str, sys.version_info[:3]))
TEMP_PATH = re.compile(re.escape(tempfile.gettempdir()) + r'/tmp[\w.-]+')


def scrub(text):
    """Drops what differs between runs, machines and patch releases: temp, repo
    and install paths, addresses, the version and build string."""
    text = text.replace(sys.version, '<sys.version>')
    text = text.replace(PATCH_VERSION, '<version>')
    text = TEMP_PATH.sub('<tmp>', text)
    text = text.replace(REPO_ROOT, '<repo>')
    text = text.replace(sys.base_prefix, '<python>')
    text = ADDRESS.sub('0x…', text)
    return text.encode('utf-8', 'backslashreplace').decode('utf-8')


def clip(text, width=100):
    text = text.replace('\n', '\\n')
    return text if len(text) <= width else text[: width - 1] + '…'


def clip_pair(want, got, width=70):
    """Both texts clipped to `width`, starting near where they first differ."""
    want, got = want.replace('\n', '\\n'), got.replace('\n', '\\n')
    common = len(os.path.commonprefix([want, got]))
    if common > width // 2:
        start = common - width // 2
        want, got = '…' + want[start:], '…' + got[start:]
    return clip(want, width), clip(got, width)


def summarise(failure):
    kind, expected, actual = failure.kind, failure.expected, failure.actual
    # Kinds are `<engine> <what>`: `CPython result`, `micro-monty traceback`, ...
    what = kind.partition(' ')[2]
    if what == 'unexpected exception':
        last = next((line for line in reversed(actual.splitlines()) if line.strip()), actual)
        return f'unexpected exception: {clip(last.strip())}'
    if what == 'exception':
        return f'no exception raised, expected {clip(expected)}'
    if what == 'traceback':
        if not actual:
            return 'no exception raised, expected a traceback'
        want, got = expected.splitlines(), actual.splitlines()
        for number, (a, b) in enumerate(itertools.zip_longest(want, got, fillvalue='<none>'), 1):
            if a != b:
                want, got = clip_pair(a.strip(), b.strip())
                return f'traceback line {number}: expected {want}, got {got}'
    if what == 'result':
        want, got = clip_pair(expected, actual)
        return f'expected {want}, got {got}'
    return f'{kind}: {clip(actual)}'


def record(name, status, expectation='', header=None, kind='', reason='', expected='', actual='', category=''):
    result = {
        'file': name,
        'status': status,
        'kind': kind,
        'reason': scrub(reason),
        'expected': scrub(expected),
        'actual': scrub(actual),
        'expectation': expectation,
        'header': header or {},
    }
    if category:
        result['category'] = category
    return result


# -- The micro-monty side (`--engine micro-monty`) ------------------------------
#
# Instead of executing a case in CPython, run it through `mm exec --json` and
# check the same expectations against what mm reports, as the Monty side of the
# datatest (`try_run_test`) does: the case's code minus its expectation (the
# last-line comment or the TRACEBACK docstring) runs as one module named after
# the case file; its final expression statement is the value `Return=`,
# `Return.str=` and `Return.type=` compare (None when it ends otherwise).

# Monty's datatest runs every case with `max_recursion_depth(50)`, which counts
# function frames but not the module frame; mm's limit counts the module frame
# too (as CPython's does), so the equivalent mm limit is one more.
MM_RECURSION_LIMIT = TEST_RECURSION_LIMIT + 1

# Seconds of its own CPU time one `mm` process may use before the case counts
# as a timeout (`RLIMIT_CPU`, so other processes on a loaded machine do not use
# it up). A process that is not running at all (stopped, starved) still ends
# after MM_WALL_FACTOR times as much wall-clock time; xtask's per-case
# watchdogs allow for it.
MM_TIMEOUT = 10.0
MM_WALL_FACTOR = 6

MM_UNSUPPORTED = re.compile(r'micro-monty does not support (.*)')


class MmAbnormal(Exception):
    """mm did not report a result: `crash` (panic, signal, bad exit or no JSON) or `timeout`."""

    def __init__(self, status, reason, detail):
        super().__init__(reason)
        self.status = status
        self.reason = reason
        self.detail = detail


def run_limited(command, cwd, timeout, zone):
    """Runs one mm process with `timeout` seconds of CPU time (MM_TIMEOUT);
    returns the finished process, or raises MmAbnormal('timeout')."""
    import math
    import resource
    import signal
    import subprocess

    cpu = max(1, math.ceil(timeout))

    def limit():
        # SIGXCPU at `cpu` seconds, SIGKILL a second later; no core file.
        resource.setrlimit(resource.RLIMIT_CPU, (cpu, cpu + 1))
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

    wall = timeout * MM_WALL_FACTOR
    try:
        proc = subprocess.run(
            command, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True, timeout=wall,
            env=mm_env(zone), preexec_fn=limit
        )
    except subprocess.TimeoutExpired as expired:
        stderr = (expired.stderr or b'').decode('utf-8', 'replace')
        raise MmAbnormal('timeout', f'mm ran past {wall:g} s of wall-clock time', stderr) from None
    if proc.returncode == -signal.SIGXCPU:
        stderr = proc.stderr.decode('utf-8', 'replace')
        raise MmAbnormal('timeout', f'mm used more than {cpu} s of CPU time', stderr)
    return proc


def mm_module_flags(expectation, header):
    """`--main-module` where CPython runs the case as `__main__`: traceback
    cases (`runpy.run_path(..., run_name='__main__')`) and the
    `cpython-main-module` header; other cases run in a bare `exec` namespace
    on both sides (`mm exec`'s default)."""
    return ['--main-module'] if header['cpython-main-module'] or expectation[0] == 'Traceback' else []


def mm_env(zone):
    """The environment mm runs in: `TZ` as `apply_timezone` sets it for
    CPython (the header's zone, or UTC), which `mm` reads as libc does."""
    return dict(os.environ, TZ=zone or 'UTC')


def run_mm(mm, name, code, timeout, fixtures=False, module_flags=(), zone=None):
    """Runs `code` as `<scratch>/<name>` through `mm exec --json`; returns (report, path).

    `mm` is the mm binary, or (wasmtime, module) for mm built for
    wasm32-wasip1 under wasmtime (`--wasm`).

    `fixtures` gives the case mm's emulation of the datatest's host fixtures
    (`--host-fixtures`), as `call-external` cases need; `module_flags` are
    `mm_module_flags`; `zone` is the header's `timezone`."""
    scratch = os.path.realpath(tempfile.mkdtemp())
    try:
        path = os.path.join(scratch, name)
        with open(path, 'w', encoding='utf-8', newline='') as f:
            f.write(code)
        args = ['exec', '--json', '--recursion-limit', str(MM_RECURSION_LIMIT)]
        if fixtures:
            args.append('--host-fixtures')
        args.extend(module_flags)
        args.append(name)
        command = wasm_command(*mm, scratch, args, zone) if isinstance(mm, tuple) else [mm, *args]
        proc = run_limited(command, scratch, timeout, zone)
        stdout = proc.stdout.decode('utf-8', 'replace')
        stderr = proc.stderr.decode('utf-8', 'replace')
        if proc.returncode != 0:
            raise MmAbnormal('crash', describe_crash(proc.returncode, stderr), stderr)
        try:
            report = json.loads(stdout)
        except ValueError:
            raise MmAbnormal('crash', 'mm exec --json printed no JSON report', stdout + stderr) from None
        return report, path
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


def describe_crash(returncode, stderr):
    """A stable one-line description: the panic message without its source location."""
    import signal

    lines = stderr.splitlines()
    for index, line in enumerate(lines):
        if ' panicked at ' in line:
            message = next((later.strip() for later in lines[index + 1 :] if later.strip()), '')
            return f'mm panicked: {clip(message)}'
        if 'has overflowed its stack' in line:
            return 'mm overflowed its stack'
    if returncode < 0:
        try:
            return f'mm killed by {signal.Signals(-returncode).name}'
        except ValueError:
            return f'mm killed by signal {-returncode}'
    last = next((line.strip() for line in reversed(lines) if line.strip()), '')
    return f'mm exited with status {returncode}: {clip(last)}'


def format_mm_exception(error):
    """`format_cpython_exception` for an exception mm reports as type and `str()`.

    mm reports `str(exc)`, not `args[0]`; they differ for KeyError, whose `str`
    is the key's repr, so its key is read back from that repr (a non-str key
    gives `KeyError()`, as on CPython).
    """
    import ast

    exc_type, message = error['type'], error['message']
    if exc_type == 'KeyError':
        try:
            key = ast.literal_eval(message)
        except (ValueError, SyntaxError, MemoryError, RecursionError):
            key = None
        message = key if isinstance(key, str) else ''
    if not message:
        return f'{exc_type}()'
    if "'" in message:
        return f'{exc_type}("{message}")'
    return f"{exc_type}('{message}')"


TRACEBACK_HEADER = 'Traceback (most recent call last):\n'


def normalise_mm_traceback(text, path, name):
    """The CPython side's normalisation (`run_traceback.py`): the file shown by
    its base name, `normalize_debug_range` per line, trailing space stripped.

    mm reports a compile error as `python FILE` prints one, without a
    `Traceback` header; the datatest's CPython side compiles inside
    `runpy.run_path`, which adds one (and Monty's parse errors carry it), so a
    report without the header gets it.
    """
    run_traceback = import_script('run_traceback')
    if not text.startswith(TRACEBACK_HEADER):
        text = TRACEBACK_HEADER + text
    text = text.replace(path, name)
    return '\n'.join(map(run_traceback.normalize_debug_range, text.splitlines())).rstrip()


def try_run_mm_test(mm, name, code, expectation, timeout, fixtures=False, module_flags=(), zone=None):
    """`try_run_test` on mm: returns None on success, raises Failure (with
    `mm_error`, mm's exception or None) or MmAbnormal otherwise."""
    kind, expected = expectation
    report, path = run_mm(mm, name, code, timeout, fixtures, module_flags, zone)
    error = report.get('error')
    failure = None
    if error is None:
        if kind in ('Return', 'ReturnStr', 'ReturnType'):
            value = report.get('result') or {'repr': 'None', 'str': 'None', 'type': 'NoneType'}
            actual = value[{'Return': 'repr', 'ReturnStr': 'str', 'ReturnType': 'type'}[kind]]
            if actual != expected:
                failure = Failure('micro-monty result', expected, actual)
        elif kind in ('Raise', 'Traceback'):
            failure = Failure('micro-monty exception', expected, 'no exception raised')
    elif kind == 'Raise':
        actual = format_mm_exception(error)
        if actual != expected:
            failure = Failure('micro-monty result', expected, actual)
    elif kind == 'Traceback':
        actual = normalise_mm_traceback(error['traceback'], path, name)
        if actual != expected:
            failure = Failure('micro-monty traceback', expected, actual)
    else:
        failure = Failure(
            'micro-monty unexpected exception',
            'no exception' if kind == 'NoException' else 'success',
            normalise_mm_traceback(error['traceback'], path, name),
        )
    if failure is not None:
        failure.mm_error = error
        raise failure


def categorise_mm_failure(failure):
    """The first cause of a failing case, for ordering the work: an unsupported
    feature, a 64-bit int limit, the cap on one int operation's work or a
    missing builtin wherever mm reports one, else what differs."""
    error = failure.mm_error
    if error is not None:
        message = error['message']
        unsupported = MM_UNSUPPORTED.search(message)
        if unsupported:
            feature = re.sub(r'\s+\(.*\)$| yet$', '', unsupported.group(1).strip())
            if feature == 'integers this large':
                return 'bigint'
            if feature.startswith('async'):
                feature = 'async'
            return f'does not support {feature}'
        if 'limited to 64 bits' in message:
            return 'bigint'
        if error['type'] == 'MemoryError' and message == 'integer operation too large':
            return 'int work cap'
        missing = re.fullmatch(r"name '(\w+)' is not defined", message)
        if error['type'] == 'NameError' and missing and hasattr(builtins, missing.group(1)):
            return 'missing builtin'
    what = failure.kind.partition(' ')[2]
    if what == 'unexpected exception':
        return f'unexpected {error["type"]}'
    if what == 'exception':
        return 'no exception raised'
    if what == 'traceback':
        return 'wrong traceback'
    return 'wrong output' if error is None else 'wrong exception'


def run_mm_case(name, code, expectation, header, mm, timeout):
    if name.startswith('refcount__'):
        return record(name, 'skip', expectation[0], header, reason='excluded: micro-monty has no reference counting')
    if header['mount-fs']:
        return record(name, 'skip', expectation[0], header, reason='needs the mount-fs host filesystem')
    if expectation[0] == 'RefCounts':
        return record(name, 'skip', expectation[0], header, reason='ref-counts expectations are Monty-only')
    try:
        try_run_mm_test(
            mm,
            name,
            code,
            expectation,
            timeout,
            header['call-external'],
            mm_module_flags(expectation, header),
            header['timezone'],
        )
    except MmAbnormal as abnormal:
        return record(
            name,
            abnormal.status,
            expectation[0],
            header,
            f'micro-monty {abnormal.status}',
            abnormal.reason,
            actual=abnormal.detail,
            category=abnormal.status,
        )
    except Failure as failure:
        failure.expected, failure.actual = scrub(failure.expected), scrub(failure.actual)
        return record(
            name,
            'fail',
            expectation[0],
            header,
            failure.kind,
            summarise(failure),
            failure.expected,
            failure.actual,
            categorise_mm_failure(failure),
        )
    return record(name, 'pass', expectation[0], header)


# -- Durability (`--durability N`) ----------------------------------------------


# `SnapshotError::TooLarge`, as `mm resume` reports it.
CROSS_TOO_LARGE = 'the snapshot needs more memory than allowed'


def mm_json_line(command, cwd, timeout, zone=None):
    """Runs one mm process; returns its last stdout line parsed, or raises MmAbnormal."""
    proc = run_limited(command, cwd, timeout, zone)
    stdout = proc.stdout.decode('utf-8', 'replace')
    stderr = proc.stderr.decode('utf-8', 'replace')
    if proc.returncode != 0:
        raise MmAbnormal('crash', describe_crash(proc.returncode, stderr), stderr)
    try:
        return json.loads(stdout.strip().splitlines()[-1])
    except (ValueError, IndexError):
        raise MmAbnormal('crash', 'mm printed no JSON line', stdout + stderr) from None


def report_of(doc):
    return {key: doc.get(key) for key in ('stdout', 'result', 'error', 'exit')}


def first_difference(want, got):
    """A short description of where two reports differ."""
    for key in ('stdout', 'result', 'error', 'exit'):
        if want[key] != got[key]:
            a, b = json.dumps(want[key]), json.dumps(got[key])
            at = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
            return f'{key} differs at char {at}: want {clip(a[max(0, at - 20):], 60)} got {clip(b[max(0, at - 20):], 60)}'
    return 'reports differ'


def wasm_command(wasmtime, module, scratch, args, zone=None):
    """`mm args` as the wasm32-wasip1 module under wasmtime. WASI has no
    working directory, so the scratch directory is preopened at its own path
    and the case file is passed absolute, as native mm resolves it. wasmtime
    passes no environment through, so `TZ` is set as `mm_env` sets it."""
    command = [wasmtime, 'run', '--dir', f'{scratch}::{scratch}']
    if os.path.isdir('/usr/share/zoneinfo'):
        command += ['--dir', '/usr/share/zoneinfo::/usr/share/zoneinfo']
    command += ['--env', f'TZ={zone or os.environ.get("TZ") or "UTC"}']
    if os.environ.get('MM_GC_STRESS'):
        command += ['--env', f'MM_GC_STRESS={os.environ["MM_GC_STRESS"]}']
    if module.endswith('.cwasm'):
        command.append('--allow-precompiled')
    command.append(module)
    for a in args:
        command.append(os.path.join(scratch, a) if a.endswith('.py') else a)
    return command


def durability_diffs(plain, got):
    """The columns in which run `got` differs from the unparked run `plain`."""
    return [column for column, same in (
        ('report', report_of(got) == report_of(plain)),
        ('fuel', got.get('fuel') == plain.get('fuel')),
        ('digest', got.get('digest') == plain.get('digest')),
    ) if not same]


def run_durability_case(path, mm, every, max_parks, timeout, alternate=None):
    """`alternate`: None, or (wasmtime, module, wasm_first) for a cross-target chain."""
    name = os.path.basename(path)
    with open(path, encoding='utf-8', newline='') as f:
        content = f.read()
    # Own cases are scripts (`mm run`, checked against `python3.14 FILE`).
    flags = ['--main-module']
    code = content
    zone = None
    if os.path.basename(os.path.dirname(os.path.realpath(path))) == 'monty':
        try:
            code, expectation, header = parse_fixture(content)
        except HeaderError as error:
            return {'file': name, 'status': 'skip', 'parks': 0, 'reason': f'invalid header: {error}'}
        if name.startswith('refcount__') or header['mount-fs'] or expectation[0] == 'RefCounts':
            return {'file': name, 'status': 'skip', 'parks': 0, 'reason': 'not run on micro-monty'}
        flags = ['--recursion-limit', str(MM_RECURSION_LIMIT)]
        if header['call-external']:
            flags.append('--host-fixtures')
        flags.extend(mm_module_flags(expectation, header))
        zone = header['timezone']
    scratch = os.path.realpath(tempfile.mkdtemp())
    parks = []
    runs = {}
    mode = 'unparked'
    try:
        with open(os.path.join(scratch, name), 'w', encoding='utf-8', newline='') as f:
            f.write(code)
        plain = mm_json_line([mm, 'exec', '--json', '--report-fuel', '--digest', *flags, name], scratch, timeout, zone)
        if max_parks:
            every = max(every, -(-plain.get('fuel', 0) // max_parks))
        # One process runs what the parked chain splits over up to about M
        # + 1 processes, each with `timeout` seconds of CPU: a snapshot at
        # each of 1,000 parks took own case 466 past 20 s [M]. The chain's
        # backstop in xtask is 15 times `timeout` too.
        long = timeout * 15
        in_process = [('parks', ['--park-every', str(every)]),
                      ('snapshot', ['--park-every', str(every), '--snapshot-continue'])]
        if alternate is None:
            for mode, extra in in_process:
                runs[mode] = mm_json_line([mm, 'exec', '--json', '--digest', *extra, *flags, name], scratch, long, zone)
        elif alternate[2]:
            for mode, extra in [('wasm', ['--report-fuel']), *(('wasm-' + m, e) for m, e in in_process)]:
                command = wasm_command(alternate[0], alternate[1], scratch,
                                       ['exec', '--json', '--digest', *extra, *flags, name], zone)
                runs[mode] = mm_json_line(command, scratch, timeout if mode == 'wasm' else long, zone)
        mode = 'cross' if alternate else 'restore'
        snap = os.path.join(scratch, 'snap')
        args = ['exec', '--json', '--digest', *flags, '--snapshot-every', str(every), '--snapshot-dir', snap, name]
        while True:
            # Process k of the chain is native unless it is wasm's turn.
            if alternate and (len(parks) + alternate[2]) % 2 == 1:
                command = wasm_command(alternate[0], alternate[1], scratch, args, zone)
            else:
                command = [mm, *args]
            try:
                line = mm_json_line(command, scratch, timeout, zone)
            except MmAbnormal as abnormal:
                # The targets charge different bytes for the same state
                # (step 4), so a park near the memory limit may not fit on
                # the other one: the chain ends there, a difference in every
                # column that only an entry allows.
                if not (alternate and parks and CROSS_TOO_LARGE in abnormal.detail):
                    raise
                line = {'stdout': parks[-1].get('stdout'), 'error': CROSS_TOO_LARGE}
                break
            if 'park' not in line:
                break
            parks.append(line)
            if len(parks) > 100_000:
                raise MmAbnormal('timeout', 'more than 100000 parks', '')
            args = ['resume', '--json', snap]
        runs[mode] = line
    except MmAbnormal as abnormal:
        where = f' after park {len(parks)}' if parks else ''
        return {'file': name, 'status': abnormal.status, 'parks': len(parks),
                'reason': f'{mode}: {abnormal.reason}{where}', 'detail': clip(abnormal.detail, 2000)}
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    want = report_of(plain)
    # Each mode against the unparked run, and the restore chain against the
    # session that took the same snapshots and went on (restore fidelity).
    pairs = {m: (plain, run) for m, run in runs.items()}
    if 'restore' in runs and 'snapshot' in runs:
        pairs['restore-vs-snapshot'] = (runs['snapshot'], runs['restore'])
    diffs = {m: d for m, (ref, run) in pairs.items() if (d := durability_diffs(ref, run))}
    fuel = {m: (run.get('fuel') or 0) - (ref.get('fuel') or 0) for m, (ref, run) in pairs.items()}
    reasons = []
    for m, d in diffs.items():
        ref, run = pairs[m]
        detail = ', '.join(d)
        if 'fuel' in d:
            detail += f' (fuel {fuel[m]:+})'
        if 'report' in d:
            if m == mode:
                # The first park whose output so far is not a prefix of the unparked run's.
                diverged = next((p['park'] for p in parks if not (want['stdout'] or '').startswith(p['stdout'])), None)
                detail += f'; output diverges by park {diverged}' if diverged else f'; after park {len(parks)}'
            detail += f': {first_difference(report_of(ref), report_of(run))}'
        reasons.append(f'{m}: {detail}')
    return {'file': name, 'status': 'differs' if diffs else 'pass', 'parks': len(parks), 'every': every,
            'diffs': diffs, 'fuel': fuel, 'reason': '; '.join(reasons)}


def run_case(path, mm=None, timeout=MM_TIMEOUT):
    name = os.path.basename(path)
    abs_path = os.path.realpath(path)
    try:
        with open(abs_path, encoding='utf-8', newline='') as f:
            content = f.read()
    except (OSError, UnicodeDecodeError) as error:
        return record(name, 'error', kind='read', reason=f'cannot read the case: {error}')
    try:
        code, expectation, header = parse_fixture(content)
    except HeaderError as error:
        return record(name, 'error', kind='header', reason=f'invalid `# /// monty` header: {error}')
    if mm is not None:
        return run_mm_case(name, code, expectation, header, mm, timeout)
    if expectation[0] == 'RefCounts':
        return record(
            name, 'skip', 'RefCounts', header, reason='ref-counts expectations are Monty-only; the datatest skips them'
        )
    if os.name == 'nt' and (header['skip-cpython-windows'] or header['timezone']):
        return record(name, 'skip', expectation[0], header, reason='skipped on Windows CPython')
    case_recursion_limit[0] = ORACLE_RECURSION_LIMIT
    scratch = tempfile.mkdtemp()
    cwd = os.getcwd()
    os.chdir(scratch)
    try:
        try_run_cpython_test(abs_path, code, expectation, header)
    except Failure as failure:
        # Scrubbed before `summarise` clips them, so nothing is left half-replaced.
        failure.expected, failure.actual = scrub(failure.expected), scrub(failure.actual)
        return record(
            name, 'fail', expectation[0], header, failure.kind, summarise(failure), failure.expected, failure.actual
        )
    except HeaderError as error:
        return record(name, 'error', expectation[0], header, 'harness', str(error))
    finally:
        os.chdir(cwd)
        shutil.rmtree(scratch, ignore_errors=True)
    return record(name, 'pass', expectation[0], header)


def run_case_guarded(path, mm=None, timeout=MM_TIMEOUT):
    try:
        return run_case(path, mm, timeout)
    except BaseException as error:  # noqa: BLE001 - an oracle bug must not lose the case
        import traceback

        detail = ''.join(traceback.format_exception(error))
        return record(os.path.basename(path), 'error', kind='oracle', reason=f'oracle error: {error!r}', actual=detail)


def prepare_interpreter():
    """What the embedded interpreter looks like when the datatest's first case runs."""
    tempfile.gettempdir()  # cache it before test_fixtures swaps os.environ
    sys.argv = ['']
    main = types.ModuleType('__main__')
    main.__builtins__ = builtins
    sys.modules['__main__'] = main
    # `ensure_python_modules_imported`, plus what the watchdog imports per case.
    for module in ('typing', 'dataclasses', 'pathlib', 'stat', 'asyncio', 'traceback', 'threading'):
        __import__(module)
    import_script('test_fixtures')
    import_script('cpython_watchdog')


USAGE = """\
usage: python3.14 conformance/oracle.py CASE.py [CASE.py ...]
       python3.14 conformance/oracle.py --engine micro-monty [--mm MM | --wasm MODULE --wasmtime WASMTIME]
           [--timeout S] CASE.py [...]
       python3.14 conformance/oracle.py --engine micro-monty --durability N[:M] [--mm MM] [--timeout S]
           [--alternate MODULE --wasmtime WASMTIME [--alternate-first native|wasm]] CASE.py [...]"""


def main(argv):
    args = argv[1:]
    engine, mm, timeout = 'cpython', os.environ.get('MM') or None, MM_TIMEOUT
    durability = None
    max_parks = None
    alternate, wasmtime, wasm_first, wasm = None, None, 0, False
    while args and args[0].startswith('--') and len(args) > 1:
        option, value, args = args[0], args[1], args[2:]
        if option == '--engine' and value in ('cpython', 'micro-monty'):
            engine = value
        elif option == '--mm':
            mm = value
        elif option == '--timeout':
            timeout = float(value)
        elif option in ('--alternate', '--wasm'):
            alternate = os.path.realpath(value)
            wasm = option == '--wasm'
        elif option == '--wasmtime':
            wasmtime = value
        elif option == '--alternate-first' and value in ('native', 'wasm'):
            wasm_first = int(value == 'wasm')
        elif option == '--durability' and re.fullmatch(r'[1-9]\d*(:[1-9]\d*)?', value):
            durability, _, cap = value.partition(':')
            durability, max_parks = int(durability), int(cap) if cap else None
        else:
            print(USAGE, file=sys.stderr)
            return 2
    paths = args
    if (
        not paths
        or paths[0] in ('-h', '--help')
        or (durability is not None and engine != 'micro-monty')
        or (alternate is not None and not wasmtime)
        or (alternate is not None and (durability is None) != wasm)
    ):
        print(USAGE, file=sys.stderr)
        return 2
    if engine == 'micro-monty' and wasm:
        for path in paths:
            print(json.dumps(run_case_guarded(path, (wasmtime, alternate), timeout)), flush=True)
        return 0
    if engine == 'micro-monty':
        if not mm:
            print('oracle.py: --engine micro-monty needs --mm or $MM (the mm binary)', file=sys.stderr)
            return 2
        mm = os.path.realpath(mm)
        if durability is not None:
            for path in paths:
                cross = (wasmtime, alternate, wasm_first) if alternate else None
                result = run_durability_case(path, mm, durability, max_parks, timeout, cross)
                print(json.dumps(result), flush=True)
            return 0
        # Each case is its own mm process; nothing runs in this interpreter.
        for path in paths:
            print(json.dumps(run_case_guarded(path, mm, timeout)), flush=True)
        return 0
    out = os.fdopen(os.dup(1), 'w', encoding='utf-8')
    devnull = os.open(os.devnull, os.O_WRONLY)
    os.dup2(devnull, 1)
    os.close(devnull)
    prepare_interpreter()
    if len(paths) == 1:
        out.write(json.dumps(run_case_guarded(paths[0])) + '\n')
        out.flush()
        return 0
    for path in paths:
        read_end, write_end = os.pipe()
        pid = os.fork()
        if pid == 0:
            os.close(read_end)
            try:
                with os.fdopen(write_end, 'w', encoding='utf-8') as pipe:
                    pipe.write(json.dumps(run_case_guarded(path)) + '\n')
            finally:
                os._exit(0)
        os.close(write_end)
        with os.fdopen(read_end, encoding='utf-8') as pipe:
            line = pipe.read()
        _, status = os.waitpid(pid, 0)
        if not line.strip():
            line = json.dumps(
                record(os.path.basename(path), 'error', kind='oracle', reason=f'case process died (status {status})')
            ) + '\n'
        out.write(line)
        out.flush()
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
