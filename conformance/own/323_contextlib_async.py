# contextlib's async helpers: asynccontextmanager, aclosing,
# AsyncContextDecorator and AsyncExitStack under `async with`.
import asyncio
from contextlib import (AsyncContextDecorator, AsyncExitStack, aclosing,
                        asynccontextmanager, nullcontext)

events = []


@asynccontextmanager
async def resource(name, fail=False):
    events.append(f'open {name}')
    await asyncio.sleep(0)
    try:
        yield name.upper()
    except KeyError as e:
        events.append(f'{name} saw {e!r}')
        if not fail:
            raise
    finally:
        await asyncio.sleep(0)
        events.append(f'close {name}')


@asynccontextmanager
async def no_yield():
    if False:
        yield


@asynccontextmanager
async def two_yields():
    yield 1
    yield 2


@asynccontextmanager
async def swallow():
    try:
        yield
    except Exception as e:
        events.append(f'swallowed {type(e).__name__}')


class Timer(AsyncContextDecorator):
    async def __aenter__(self):
        events.append('timer start')
        return self

    async def __aexit__(self, *exc):
        events.append(f'timer stop {exc[0].__name__ if exc[0] else None}')
        return False


@Timer()
async def timed(x):
    await asyncio.sleep(0)
    return x * 2


@resource('deco')
async def decorated():
    events.append('in decorated')
    return 'decorated result'


async def numbers():
    try:
        for i in range(10):
            yield i
    finally:
        events.append('numbers closed')


class Res:
    def __init__(self, name):
        self.name = name

    async def __aenter__(self):
        events.append(f'enter {self.name}')
        return self.name

    async def __aexit__(self, *exc):
        events.append(f'exit {self.name} {exc[0].__name__ if exc[0] else None}')


async def main():
    async with resource('a') as a, resource('b') as b:
        events.append(a + b)
    try:
        async with resource('c'):
            raise KeyError('k')
    except KeyError as e:
        events.append(f'propagated {e!r}')
    async with resource('d', fail=True):
        raise KeyError('swallowed')
    try:
        async with no_yield():
            pass
    except RuntimeError as e:
        events.append(str(e))
    try:
        async with two_yields():
            pass
    except RuntimeError as e:
        events.append(str(e))
    async with swallow():
        raise ValueError
    events.append(await timed(21))
    events.append(await decorated())
    async with aclosing(numbers()) as nums:
        async for n in nums:
            if n == 2:
                break
    events.append(n)
    async with AsyncExitStack() as stack:
        names = [await stack.enter_async_context(Res(n)) for n in 'xyz']
        stack.push_async_callback(asyncio.sleep, 0)
        stack.callback(events.append, 'sync callback')
        events.append(names)
    try:
        async with AsyncExitStack() as stack:
            await stack.enter_async_context(Res('p'))
            await stack.enter_async_context(resource('q'))
            raise KeyError('in stack')
    except KeyError as e:
        events.append(f'stack propagated {e!r}')
    async with nullcontext(5) as v:
        events.append(v)
    try:
        await AsyncExitStack().enter_async_context(object())
    except TypeError as e:
        events.append(str(e))


asyncio.run(main())
for e in events:
    print(e)
