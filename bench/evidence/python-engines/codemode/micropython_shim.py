# One codemode solution on MicroPython (unix port): conformance/codemode/shim.py's
# job, in what MicroPython runs. It speaks the runner's line protocol on
# stdin/stdout (conformance/codemode/run.py `run_one`): an init line, a `call`
# line per tool call answered by the harness, then `done`.
#
# Tools are `async def name(arguments)` around a blocking call to the harness,
# as an embedder whose host answers synchronously would bind them: MicroPython
# cannot hand control back to the host with a call outstanding, so an
# `asyncio.gather` of N tools makes N calls one after another.
# The clock is pinned to the init's `clock` (time.time, localtime, the
# micropython-lib datetime that reads them) and sleeps are virtual, as on the
# CPython shim. A solution with top-level `await` arrives wrapped by
# capability.py (`async def` + `global` + `asyncio.run`), since MicroPython
# compiles no top-level await.
# Tool failures raise the exception type the tool names; the types MicroPython
# lacks (TimeoutError, PermissionError, FileNotFoundError, ConnectionError) are
# defined here as OSError subclasses, as an embedder's host-error classes.
import sys
import json
import builtins

PROTO_IN = sys.stdin
PROTO_OUT = sys.stdout


def _read():
    return json.loads(PROTO_IN.readline())


init = _read()
CLOCK = init['clock']
_state = {'offset': 0.0, 'calls': 0, 'id': 0}

import time as _rt


class _FakeTime:
    pass


_ft = _FakeTime()
for _k in dir(_rt):
    if not _k.startswith('__'):
        setattr(_ft, _k, getattr(_rt, _k))


def _now():
    return CLOCK + _state['offset']


def _sleep(secs):
    _state['offset'] += max(0.0, float(secs))


_ft.time = _now
_ft.time_ns = lambda: int(_now() * 1e9)
_ft.sleep = _sleep
_ft.sleep_ms = lambda ms: _sleep(ms / 1000)
_ft.localtime = lambda t=None: _rt.localtime(int(_now() if t is None else t))
_ft.gmtime = lambda t=None: _rt.gmtime(int(_now() if t is None else t))
sys.modules['time'] = _ft

import asyncio

_real_sleep = asyncio.sleep


async def _async_sleep(delay, result=None):
    _sleep(delay)
    await _real_sleep(0)
    return result


asyncio.sleep = _async_sleep

_EXC = {}
for _n in ('TimeoutError', 'PermissionError', 'FileNotFoundError', 'ConnectionError'):
    if hasattr(builtins, _n):
        _EXC[_n] = getattr(builtins, _n)
    else:
        _EXC[_n] = type(_n, (OSError,), {})


def _exc(kind):
    if kind in _EXC:
        return _EXC[kind]
    return getattr(builtins, kind, RuntimeError)


def encode(v):
    if v is None or isinstance(v, (bool, str)):
        return v
    if isinstance(v, int):
        return v if -(2 ** 63) <= v < 2 ** 63 else {'$i': str(v)}
    if isinstance(v, float):
        return {'$f': repr(v)}
    if isinstance(v, list):
        return [encode(x) for x in v]
    if isinstance(v, tuple):
        return {'$t': [encode(x) for x in v]}
    if isinstance(v, dict):
        return {'$d': [[k, encode(x)] for k, x in v.items()]}
    if isinstance(v, (bytes, bytearray)):
        return {'$b': bytes(v).hex()}
    raise TypeError('Object of type %s is not JSON serializable' % type(v).__name__)


def decode(v):
    if isinstance(v, list):
        return [decode(x) for x in v]
    if isinstance(v, dict):
        if '$f' in v:
            return float(v['$f'])
        if '$i' in v:
            return int(v['$i'])
        if '$t' in v:
            return tuple(decode(x) for x in v['$t'])
        if '$d' in v:
            return {k: decode(x) for k, x in v['$d']}
        if '$b' in v:
            return bytes.fromhex(v['$b'])
    return v


def _call(name, arguments):
    _state['id'] += 1
    _state['calls'] += 1
    arguments = json.loads(json.dumps(arguments))
    line = {'call': _state['id'], 'name': name, 'args': [encode(arguments)], 'kwargs': [], 'batch': 0}
    PROTO_OUT.write(json.dumps(line) + '\n')
    return _read()


def _bind(name):
    async def tool(arguments=None):
        reply = _call(name, arguments)
        await _real_sleep(0)
        if 'ok' in reply:
            return decode(reply['ok'])
        kind, message = reply['err']
        raise _exc(kind)(message)

    return tool


_out = []


def _print(*args, sep=' ', end='\n', file=None, flush=False):
    text = sep.join(str(a) for a in args) + end
    if file is not None and file is not sys.stdout and file is not sys.stderr:
        file.write(text)
    else:
        _out.append(text)


namespace = {'__name__': '__main__', 'print': _print}
namespace.update(_EXC)
for _t in init['tools']:
    namespace[_t] = _bind(_t)

error = None
try:
    exec(compile(init['source'], 'solution.py', 'exec'), namespace)
except BaseException as e:
    try:
        import io
        buf = io.StringIO()
        sys.print_exception(e, buf)
        tb = buf.getvalue()
    except BaseException:
        tb = ''
    error = {'type': type(e).__name__, 'message': str(e), 'traceback': tb}
done = {'stdout': ''.join(_out), 'stderr': '', 'error': error, 'calls': _state['calls']}
PROTO_OUT.write(json.dumps({'done': done}) + '\n')
