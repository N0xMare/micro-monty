# A coroutine or future used where a value is expected keeps CPython's
# TypeError or AttributeError: its message and args are CPython's, and so
# is the report but for one sentence micro-monty appends to the last line,
# as it appends its redirect hints (docs/user-guide.md "Did you forget
# 'await'"). The rendered lines are compared with that sentence removed;
# tests/sessions.rs checks that micro-monty adds it. Awaited values, async
# generators and the messages below that name no wrong value get no
# sentence, so their lines are compared whole.
import asyncio
import json
import re
import traceback

HINT = ". Did you forget 'await'?"


async def fetch():
    return '{"k": 1}'


async def numbers():
    yield 1


def line(e):
    return traceback.format_exception_only(e)[-1].rstrip('\n')


def show(label, f):
    try:
        f()
    except (TypeError, AttributeError) as e:
        print(label, type(e).__name__, '|', e, '|', e.args)
        print(label, line(e).removesuffix(HINT))


c = fetch()
show('json.loads', lambda: json.loads(c))
show('len', lambda: len(c))
show('subscript', lambda: c['k'])
show('add', lambda: c + 1)
show('radd', lambda: 1 + c)
show('compare', lambda: c < 1)
show('attribute', lambda: c.get('k'))
show('iterate', lambda: [x for x in c])
show('call', lambda: c())
show('int', lambda: int(c))
show('join', lambda: ','.join([c]))
show('format', lambda: format(c, '>3'))
show('concatenate str', lambda: 'n: ' + c)
show('concatenate list', lambda: [1] + c)
show('concatenate tuple', lambda: (1,) + c)
show('concatenate bytes', lambda: b'a' + c)
show('json.dumps', lambda: json.dumps(c))
show('round', lambda: round(c))
show('negate', lambda: -c)
show('contains', lambda: 1 in c)
show('list index', lambda: [1][c])
show('str index', lambda: 'ab'[c])
show('float', lambda: float(c))
show('re.search', lambda: re.search('a', c))
show('repeat', lambda: 'x' * c)
show('bytes', lambda: bytes(c))


def unpack():
    a, b = c


show('unpack', unpack)
c.close()


def whole(label, f):
    try:
        f()
    except (TypeError, AttributeError) as e:
        print(label, line(e))


async def main():
    value = await fetch()
    whole('awaited subscript', lambda: value['k'] + 1)
    whole('awaited len', lambda: len(json.loads(value)) + value)
    fut = asyncio.get_running_loop().create_future()
    fut.set_result(3)
    got = await fut
    whole('awaited future', lambda: len(got))
    agen = numbers()
    whole('async generator', lambda: len(agen))
    whole('async generator subscript', lambda: agen[0])
    gathered = asyncio.gather(fetch(), fetch())
    show('gather subscript', lambda: gathered[0])
    show('gather len', lambda: len(gathered))
    await gathered
    c = fetch()
    try:
        async for x in c:
            pass
    except TypeError as e:
        print('async for', line(e).removesuffix(HINT))
    c.close()
    c = fetch()
    try:
        await asyncio.wait(c)
    except TypeError as e:
        print('wait', line(e))
    c.close()


asyncio.run(main())


def kw(x):
    return x


def named(*, coroutine=1):
    return coroutine


def both(coroutine, Future):
    return coroutine


def keyword_only(*, Future):
    return Future


class OwnTypeError(TypeError):
    def __str__(self):
        return 'its own text'


def own():
    raise OwnTypeError("'coroutine' object is not subscriptable")


def long_message():
    raise TypeError("'coroutine' object is not subscriptable" + ' ' * 2000)


class MyFuture:
    pass


class coroutine_like:
    pass


def send_first():
    c = fetch()
    try:
        c.send(1)
    finally:
        c.close()


# The same words naming no value's type: a keyword, a parameter, an
# attribute, a type wanted, a coroutine's own state; and messages not
# matched: a subclass's own text, one past 256 bytes; and class names that
# contain the awaitables' names.
whole('keyword', lambda: kw(coroutine=1))
whole('keyword typo', lambda: named(coroutin=1))
whole('missing', lambda: both())
whole('missing one', lambda: both(1))
whole('missing keyword-only', lambda: keyword_only())
whole('attribute name', lambda: {}.coroutine)
whole('wanted', lambda: asyncio.ensure_future(1))
whole('send', send_first)
whole('own class', own)
whole('long', long_message)
for k in (MyFuture(), coroutine_like()):
    whole('contains subscript', lambda: k['a'])
    whole('contains got', lambda: re.search('a', k))
    whole('contains repeat', lambda: 'x' * k)
