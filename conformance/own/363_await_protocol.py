# await on user awaitables (__await__ returning a generator, an iterator, a
# coroutine's __await__()), types.coroutine generators, coroutines driven by
# send()/throw()/close() through nested awaits, the __await__ result errors,
# yield from a coroutine in a types.coroutine generator, and bare yields under
# asyncio (None yields to the loop; any other value is an error).
import types

def run_async(coro):
    buffer = []
    result = None
    while True:
        try:
            buffer.append(coro.send(None))
        except StopIteration as ex:
            result = ex.args[0] if ex.args else None
            break
    return buffer, result

class AsyncYieldFrom:
    def __init__(self, obj):
        self.obj = obj
    def __await__(self):
        yield from self.obj

class AsyncYield:
    def __init__(self, value):
        self.value = value
    def __await__(self):
        yield self.value

async def foo():
    await AsyncYieldFrom([1, 2, 3])
print(run_async(foo()))

class Awaitable:
    def __await__(self):
        yield 42
        return 100
async def foo2():
    return (await Awaitable())
print(run_async(foo2()))

class A6:
    def __await__(self):
        return iter([52])
async def foo6():
    return (await A6())
print(run_async(foo6()))

for bad in (lambda self: None, lambda self: self):
    class B:
        __await__ = bad
    async def fb():
        return await B()
    try:
        run_async(fb())
    except TypeError as e:
        print("TypeError", e)

async def inner(n):
    x = await AsyncYield(n)
    y = await AsyncYield(n + 1)
    return x, y
async def outer():
    a = await inner(1)
    b = await inner(10)
    return a, b
c = outer()
print(c.send(None))
print(c.send('s1'))
print(c.send('s2'))
print(c.send('s3'))
try:
    c.send('s4')
except StopIteration as e:
    print("stop", e.value)

@types.coroutine
def nop():
    yield

async def coroutine():
    await nop()

async def waiter(coro):
    await coro

coro = coroutine()
coro.send(None)
try:
    waiter(coro).send(None)
except RuntimeError as e:
    print("RuntimeError", e)

@types.coroutine
def bar():
    yield 1
    yield 2
async def f6():
    await bar()
f = f6()
print(f.send(None), f.send(None))
try:
    f.send(None)
except StopIteration:
    print("stop")
print([el for el in bar()], tuple(bar()), next(iter(bar())))

N = 0
@types.coroutine
def gen():
    global N
    try:
        a = yield
        yield (a ** 2)
    except ZeroDivisionError:
        N += 100
        raise
    finally:
        N += 1
async def foo10():
    await gen()
coro = foo10()
aw = coro.__await__()
print(aw is iter(aw), 'coroutine_wrapper' in repr(aw), '__await__' in dir(coro))
next(aw)
print(aw.send(10))
aw.close()
print(N)
coro = foo10()
aw = coro.__await__()
next(aw)
try:
    aw.throw(ZeroDivisionError())
except ZeroDivisionError:
    print("ZDE", N)

@types.coroutine
def g14():
    yield
async def c14():
    try:
        await g14()
    except GeneratorExit:
        await g14()
c = c14()
c.send(None)
try:
    c.close()
except RuntimeError as e:
    print("RuntimeError", e)

async def coro_r():
    return 'spam'
c = coro_r()
class A12:
    def __await__(self):
        return c
async def f12():
    return await A12()
try:
    run_async(f12())
except TypeError as e:
    print("TypeError", e)
c.close()

class Wrapper:
    def __init__(self, coro):
        self.coro = coro
    def __await__(self):
        return self.coro.__await__()
class FutureLike:
    def __await__(self):
        return (yield)
class Marker(Exception):
    pass
async def coro1():
    try:
        return await FutureLike()
    except ZeroDivisionError:
        raise Marker
async def coro2():
    return await Wrapper(coro1())
c = coro2()
c.send(None)
try:
    c.send('spam')
except StopIteration as e:
    print("stop", e)
c = coro2()
c.send(None)
try:
    c.throw(ZeroDivisionError)
except Marker:
    print("Marker")

@types.coroutine
def b8():
    return (yield from coro8)
async def f8():
    return 'spam'
coro8 = f8()
print(run_async(b8()))
c7 = f8()
def g7():
    yield from c7
try:
    list(g7())
except TypeError as e:
    print("TypeError", e)
c7.close()

import asyncio
async def t1():
    await nop()
    await AsyncYield(None)
    return 'task done'
print(asyncio.run(t1()))
async def t2():
    await AsyncYield(5)
try:
    asyncio.run(t2())
except RuntimeError as e:
    print("RuntimeError", e)

class Ready:
    def __init__(self, v):
        self.v = v
    def __await__(self):
        yield
        return self.v

class Counter:
    def __init__(self, n):
        self.i, self.n = 0, n
    def __aiter__(self):
        return self
    def __anext__(self):
        self.i += 1
        if self.i > self.n:
            raise StopAsyncIteration
        return Ready(self.i * 10)

class Ctx:
    def __aenter__(self):
        return Ready('entered')
    def __aexit__(self, *exc):
        return Ready(False)

async def agen_user():
    out = [x async for x in Counter(3)]
    async with Ctx() as v:
        out.append(v)
    return out

async def task(name, n):
    total = 0
    for i in range(n):
        total += await Ready(i)
    return name, total

async def main():
    r = await asyncio.gather(task('a', 3), task('b', 5), agen_user())
    return r
print(asyncio.run(main()))
print(run_async(agen_user()))
