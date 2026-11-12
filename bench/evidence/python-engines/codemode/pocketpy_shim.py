# One codemode solution on PocketPy: conformance/codemode/shim.py's job, in
# what PocketPy runs. It speaks the runner's line protocol (conformance/
# codemode/run.py `run_one`) through `input()` and `print(..., flush=True)`,
# PocketPy's only stdio.
#
# PocketPy has no `async`/`await` (its lexer has neither keyword), so tools are
# plain functions that block on the harness. Solutions as written (`await
# tool(...)`) stop at compile time; capability.py's `pocketpy-sync` run feeds
# them through `sync_rewrite` first (a harness transform, reported apart).
# The clock is pinned (time.time, time.localtime, which PocketPy's datetime
# reads) and `time.sleep` is virtual. Tool failures raise the type the tool
# names; types PocketPy lacks (FileNotFoundError, LookupError,
# ConnectionError) are defined here (as Exception subclasses: OSError is not subclassable on PocketPy), as an embedder's host-error classes.
import json
import builtins
import time as _rt


def _read():
    return json.loads(input())


init = _read()
CLOCK = init['clock']
_state = {'offset': 0.0, 'calls': 0, 'id': 0}
_real_localtime = _rt.localtime


def _now():
    return CLOCK + _state['offset']


def _sleep(secs):
    _state['offset'] += max(0.0, float(secs))


_rt.time = _now
_rt.sleep = _sleep


def _localtime(t=None):
    if t is None:
        t = _now()
    return _real_localtime(t)


_rt.localtime = _localtime

_EXC = {}
_missing = []
for _n in ('TimeoutError', 'PermissionError', 'FileNotFoundError', 'ConnectionError', 'LookupError'):
    if hasattr(builtins, _n):
        _EXC[_n] = getattr(builtins, _n)
    else:
        _missing.append(_n)
_defs = {}
exec(''.join(['class %s(Exception):\n    pass\n' % n for n in _missing]), _defs)
for _n in _missing:
    _EXC[_n] = _defs[_n]


def _exc(kind):
    if kind in _EXC:
        return _EXC[kind]
    if hasattr(builtins, kind):
        return getattr(builtins, kind)
    return RuntimeError


def encode(v):
    if v is None or isinstance(v, (bool, str)):
        return v
    if isinstance(v, int):
        return v
    if isinstance(v, float):
        return {'$f': repr(v)}
    if isinstance(v, list):
        return [encode(x) for x in v]
    if isinstance(v, tuple):
        return {'$t': [encode(x) for x in v]}
    if isinstance(v, dict):
        return {'$d': [[k, encode(x)] for k, x in v.items()]}
    if isinstance(v, bytes):
        return {'$b': v.hex()}
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
            return tuple([decode(x) for x in v['$t']])
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
    print(json.dumps(line), flush=True)
    return _read()


def _bind(name):
    def tool(arguments=None):
        reply = _call(name, arguments)
        if 'ok' in reply:
            return decode(reply['ok'])
        raise _exc(reply['err'][0])(reply['err'][1])

    return tool


_out = []


def _print(*args, sep=' ', end='\n', flush=False):
    _out.append(sep.join([str(a) for a in args]) + end)


namespace = {'__name__': '__main__', 'print': _print}
for _k in _EXC:
    namespace[_k] = _EXC[_k]
for _t in init['tools']:
    namespace[_t] = _bind(_t)

error = None
try:
    exec(compile(init['source'], 'solution.py', 'exec'), namespace)
except BaseException as e:
    error = {'type': type(e).__name__, 'message': str(e), 'traceback': ''}
done = {'stdout': ''.join(_out), 'stderr': '', 'error': error, 'calls': _state['calls']}
print(json.dumps({'done': done}), flush=True)
