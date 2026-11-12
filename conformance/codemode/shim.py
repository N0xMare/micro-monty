"""Runs one codemode solution on CPython; the reference side of run.py.

It speaks the runner's line protocol (runner/src/main.rs) on stdin and
stdout: an init line, a `call` line per tool call answered by the harness,
then `done`. Tools are bound as in the programmatic tool calling
convention: `async def name(arguments)` returning the harness's answer,
with arguments passed through `json.dumps` as the real convention sends
them. The solution runs with top-level await when it uses it, else as a
plain script (so `asyncio.run(main())` works). The clock is pinned to the
init's `clock` (time.time, datetime.now/today, date.today) and sleeping is
virtual, as on micro-monty.
"""

import ast
import asyncio
import builtins
import datetime as _dt
import inspect
import io
import json
import sys
import time
import traceback

PROTO_IN = sys.stdin
PROTO_OUT = sys.stdout
sys.path.insert(0, __file__.rsplit('/', 1)[0])
from ptc import decode, encode  # noqa: E402

init = json.loads(PROTO_IN.readline())
CLOCK = init['clock']
offset = 0.0
call_id = 0
calls = 0
# Batches as micro-monty forms them (run.py `call_batches`). micro-monty delivers every result one `Waiting`
# collected at once, then runs the resumed code; the calls that code issues share the next batch. Here each
# tool resumes right after its own result, so a call's batch is the event-loop iteration of the latest
# delivery before it: deliveries of one iteration count as one.
tick = 0
last_delivery = 0
_run_once = asyncio.base_events.BaseEventLoop._run_once


def _counted_run_once(self):
    global tick
    tick += 1
    return _run_once(self)


asyncio.base_events.BaseEventLoop._run_once = _counted_run_once


def _now() -> float:
    return CLOCK + offset


def _sleep(secs):
    global offset
    offset += max(0.0, float(secs))


_real_sleep = asyncio.sleep


async def _async_sleep(delay, result=None):
    _sleep(delay)
    await _real_sleep(0)
    return result


time.time = _now
time.time_ns = lambda: int(_now() * 1e9)
time.sleep = _sleep
asyncio.sleep = _async_sleep


class date(_dt.date):
    @classmethod
    def today(cls):
        return cls.fromtimestamp(_now())


class datetime(_dt.datetime):
    @classmethod
    def now(cls, tz=None):
        return cls.fromtimestamp(_now(), tz)

    @classmethod
    def today(cls):
        return cls.fromtimestamp(_now())

    @classmethod
    def utcnow(cls):
        return cls.fromtimestamp(_now(), _dt.UTC).replace(tzinfo=None)


for cls in (date, datetime):
    cls.__module__ = 'datetime'
_dt.date = date
_dt.datetime = datetime


def _call(name, arguments):
    global call_id, calls
    call_id += 1
    calls += 1
    # As the convention sends them: JSON text of the arguments.
    arguments = json.loads(json.dumps(arguments))
    line = {'call': call_id, 'name': name, 'args': [encode(arguments)], 'kwargs': [], 'batch': last_delivery}
    PROTO_OUT.write(json.dumps(line) + '\n')
    PROTO_OUT.flush()
    return json.loads(PROTO_IN.readline())


def _bind(name):
    async def tool(arguments):
        reply = _call(name, arguments)
        # Results and failures both arrive after one loop turn, as a host future settles on micro-monty.
        await _real_sleep(0)
        global last_delivery
        last_delivery = tick
        if 'ok' in reply:
            return decode(reply['ok'])
        kind, message = reply['err']
        raise getattr(builtins, kind, RuntimeError)(message)

    tool.__name__ = tool.__qualname__ = name
    return tool


namespace = {'__name__': '__main__', '__builtins__': builtins}
for tool_name in init['tools']:
    namespace[tool_name] = _bind(tool_name)

out, err = io.StringIO(), io.StringIO()
error = None
sys.stdout, sys.stderr = out, err
try:
    code = compile(init['source'], 'solution.py', 'exec', flags=ast.PyCF_ALLOW_TOP_LEVEL_AWAIT)
    if code.co_flags & inspect.CO_COROUTINE:
        asyncio.run(eval(code, namespace))
    else:
        exec(code, namespace)
except BaseException as e:  # noqa: BLE001 - every outcome is reported
    tb = ''.join(traceback.format_exception(e))
    error = {'type': type(e).__name__, 'message': str(e), 'traceback': tb}
finally:
    sys.stdout, sys.stderr = sys.__stdout__, sys.__stderr__
done = {'stdout': out.getvalue(), 'stderr': err.getvalue(), 'error': error, 'calls': calls}
PROTO_OUT.write(json.dumps({'done': done}) + '\n')
PROTO_OUT.flush()
