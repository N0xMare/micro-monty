# Coroutine objects' send(), throw() and close() (docs/user-guide.md "Known differences from CPython"):
# send(None) runs a coroutine to its end and raises StopIteration with its
# value; errors for reuse, running coroutines and bad arguments follow
# CPython. No case suspends inside send(), where mm runs on instead of
# yielding to the caller.
import asyncio
import types


def show(f, *a):
    try:
        r = f(*a)
        print('value', repr(r))
    except StopIteration as e:
        print('StopIteration', repr(e.value), e.args)
    except BaseException as e:
        print(type(e).__name__, e)


async def one():
    return 1


async def none():
    pass


c = one()
show(c.send, None)
show(c.send, None)
show(c.throw, ValueError)
show(c.close)
show(none().send, None)
c = one()
show(c.send, 1)
show(c.send, None)
c = one()
show(c.throw, ValueError('x'))
show(c.send, None)
c = one()
show(c.throw, KeyError('k'))
c = one()
show(c.close)
show(c.close)
show(c.send, None)
show(c.throw, TypeError)


async def me_too():
    for f, a in ((me.send, (None,)), (me.throw, (KeyError,)), (me.close, ())):
        show(f, *a)
    return 'done'

me = me_too()
show(me.send, None)


async def inner(x):
    return x * 2


async def agen():
    for i in range(3):
        yield i


class CM:
    async def __aenter__(self):
        print('enter')
        return self

    async def __aexit__(self, t, v, tb):
        print('exit', t.__name__ if t else None, v)
        return True


async def outer():
    v = await inner(3)
    out = [i async for i in agen()]
    async with CM():
        raise KeyError('k')
    return v, out

show(outer().send, None)


async def boom():
    await inner(1)
    raise ValueError('bad')

show(boom().send, None)


async def stop_inside():
    raise StopIteration(3)

try:
    stop_inside().send(None)
except RuntimeError as e:
    print(type(e).__name__, e, repr(e.__cause__), e.__suppress_context__)


async def nested(n):
    if n == 0:
        return 0
    try:
        nested(n - 1).send(None)
    except StopIteration as e:
        return e.value + 1

show(nested(30).send, None)


def run_no_yield(fn, *args):
    coro = fn(*args)
    try:
        coro.send(None)
    except StopIteration as e:
        return e.value
    else:
        raise AssertionError('coroutine did not complete')
    finally:
        coro.close()

print(run_no_yield(inner, 21))


async def main():
    c = inner(4)
    print('inside asyncio.run', run_no_yield(inner, 4), await c)
    try:
        c.send(None)
    except RuntimeError as e:
        print(e)
    return await inner(5)

print(asyncio.run(main()))
o = one()
print(type(o.send).__name__, types.CoroutineType.send, types.CoroutineType.close.__name__)
o.close()
