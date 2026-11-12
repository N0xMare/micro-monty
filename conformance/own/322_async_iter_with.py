# async for, async with, async generators (asend/athrow/aclose), async
# comprehensions, aiter/anext and their CPython error messages.
import asyncio

log = []
class CM:
    def __init__(self, name, suppress=False):
        self.name = name; self.suppress = suppress
    async def __aenter__(self):
        log.append(f"enter {self.name}")
        await asyncio.sleep(0)
        return self.name
    async def __aexit__(self, t, e, tb):
        log.append(f"exit {self.name} {t.__name__ if t else None} {e!r} {tb is not None}")
        await asyncio.sleep(0)
        return self.suppress
async def agen(n):
    for i in range(n):
        await asyncio.sleep(0)
        yield i
async def ticker(name, n):
    async for i in agen(n):
        log.append(f"{name}{i}")
async def f1():
    async with CM("a") as a, CM("b") as b:
        log.append(a + b)
    async with CM("s", True):
        raise ValueError("boom")
    try:
        async with CM("n"):
            raise KeyError("k")
    except KeyError as e:
        log.append(f"caught {e!r}")
    for i in range(3):
        async with CM(f"l{i}"):
            if i == 0: continue
            if i == 1: break
    async with CM("r"):
        return "ret"
async def f2():
    total = 0
    async for i in agen(5):
        if i == 1: continue
        if i == 4: break
        total += i
    else:
        log.append("no else")
    async for i in agen(2):
        pass
    else:
        log.append(f"else {i}")
    async for i in agen(0):
        log.append("never")
    else:
        log.append("empty else")
    return total
async def f3():
    xs = [x * 10 async for x in agen(4) if x % 2]
    d = {x: y async for x in agen(2) async for y in agen(2)}
    nested = [[y async for y in agen(x)] async for x in agen(3)]
    g = (x + 1 async for x in agen(3))
    mixed = [(x, y) async for x in agen(2) for y in "ab"]
    aw = [await asyncio.sleep(0, i) for i in range(3)]
    return xs, d, nested, [v async for v in g], mixed, aw
async def echo():
    v = yield "start"
    try:
        while True:
            v = yield f"got {v}"
    except ValueError as e:
        yield f"caught {e}"
    finally:
        log.append("echo finally")
async def f4():
    e = echo()
    out = [await e.asend(None), await e.asend(1), await anext(e), await e.athrow(ValueError("x"))]
    try:
        await e.asend(2)
    except StopAsyncIteration:
        out.append("stop")
    e2 = echo()
    await anext(e2)
    out.append(await e2.aclose())
    out.append(await e2.aclose())
    return out
class Countdown:
    def __init__(self, n): self.n = n
    def __aiter__(self): return self
    async def __anext__(self):
        if self.n == 0:
            raise StopAsyncIteration
        self.n -= 1
        await asyncio.sleep(0)
        return self.n
async def main():
    log.append(await f1())
    log.append(await f2())
    log.append(await f3())
    log.append(await f4())
    await asyncio.gather(ticker("x", 3), ticker("y", 2))
    log.append([i async for i in Countdown(3)])
    log.append(await anext(Countdown(0), "dflt"))
    log.append(await anext(aiter(Countdown(2))))



class NoAiter: pass
class BadAiter:
    def __aiter__(self): return 1
class BadAnext:
    def __aiter__(self): return self
    def __anext__(self): return 1
class Sync:
    def __enter__(self): return self
    def __exit__(self, *a): pass
class OnlyAenter:
    async def __aenter__(self): return self
class BadAenter:
    def __aenter__(self): return 1
    async def __aexit__(self, *a): pass
class BadAexit:
    async def __aenter__(self): return 1
    def __aexit__(self, *a): return 1
async def t(f):
    try: await f()
    except Exception as e: print(type(e).__name__ + ":", e)
async def errors():
    async def a():
        async for x in NoAiter(): pass
    await t(a)
    async def a():
        async for x in BadAiter(): pass
    await t(a)
    async def a():
        async for x in BadAnext(): pass
    await t(a)
    async def a():
        async with Sync(): pass
    await t(a)
    async def a():
        async with OnlyAenter(): pass
    await t(a)
    async def a():
        async with NoAiter(): pass
    await t(a)
    async def a():
        async with BadAenter(): pass
    await t(a)
    async def a():
        async with BadAexit(): pass
    await t(a)
    async def a():
        async with BadAexit():
            raise KeyError
    await t(a)
    async def a(): print(anext(BadAnext()))
    await t(a)
    async def a(): print(await anext(BadAnext(), 5))
    await t(a)
    async def a(): print(aiter(BadAiter()))
    await t(a)
    async def a():
        async def g():
            yield 1
            raise StopAsyncIteration
        async for x in g(): pass
    await t(a)
    async def a():
        async def g():
            yield 1
            raise StopIteration
        async for x in g(): pass
    await t(a)
    async def a():
        async def g():
            try: yield 1
            except GeneratorExit: yield 2
        x = g(); await anext(x); await x.aclose()
    await t(a)
    async def a():
        async def g():
            yield 1
        x = g(); await x.asend(3)
    await t(a)
    async def a():
        async def g():
            await x.asend(None)
            yield 1
        x = g(); await x.asend(None)
    await t(a)
    async def a():
        async def g():
            yield 1
        x = g()
        for y in x: pass
    await t(a)
    async def a():
        async def g():
            yield 1
        print([y async for y in g()], [await asyncio.sleep(0, z) for z in range(2)])
        print({y: 1 async for y in g()}, {y async for y in g()})
        h = (y*2 async for y in g())
        print(type(h).__name__, [z async for z in h])
        h = (await asyncio.sleep(0, z) for z in range(3))
        print(type(h).__name__, [z async for z in h])
    await t(a)


class Async:
    async def __aenter__(self): return self
    async def __aexit__(self, *a): pass


def sync_with_async_cm():
    try:
        with Async():
            pass
    except TypeError as e:
        print('TypeError:', e)


async def agen_api():
    async def fresh():
        yield 1
    f = fresh()
    ops = (f.asend(None), f.athrow(ValueError), f.aclose())
    print(type(f).__name__, *[type(o).__name__ for o in ops], f.__name__, f.__qualname__, f.ag_running)
    print(await ops[0])
    try:
        await ops[1]
    except ValueError:
        print('athrow raised')
    print(await ops[2])
    f = fresh()
    try:
        await f.athrow(ValueError('x'))
    except ValueError as e:
        print('athrow fresh', repr(e))
    try:
        await f.__anext__()
    except StopAsyncIteration as e:
        print('then', repr(e))
    print(await fresh().aclose())
    f = fresh()
    print(await f.__anext__(), await anext(f, 'dflt'), await f.athrow(KeyError), await f.aclose())
    for make in (lambda: fresh().__anext__(), lambda: fresh().aclose()):
        aw = make()
        await aw
        try:
            await aw
        except RuntimeError as e:
            print(e)
    try:
        anext(1)
    except TypeError as e:
        print(e)
    try:
        aiter(1)
    except TypeError as e:
        print(e)
    print(aiter(f) is f)


async def run_all():
    await main()
    await errors()
    await agen_api()


asyncio.run(run_all())
sync_with_async_cm()
for line in log:
    print(line)
