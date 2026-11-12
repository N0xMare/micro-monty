# Exceptions through await and gather, coroutine reuse and bad awaits.
import asyncio


async def fail(msg, after=0):
    for _ in range(after):
        await asyncio.sleep(0)
    raise ValueError(msg)


async def ok(x):
    await asyncio.sleep(0)
    return x


async def guarded(events):
    try:
        await fail('inner')
    except ValueError as e:
        events.append(f'caught {e}')
        raise KeyError('outer') from e
    finally:
        events.append('finally')


async def main():
    try:
        await asyncio.gather(ok(1), fail('boom', 1), ok(3))
    except ValueError as e:
        print('gather raised', repr(e))
    print(await asyncio.gather(ok(1), fail('kept'), ok(3), return_exceptions=True))
    events = []
    try:
        await guarded(events)
    except KeyError as e:
        print(repr(e), repr(e.__cause__), events)
    c = ok('once')
    print(await c)
    try:
        await c
    except RuntimeError as e:
        print('reuse:', e)
    for bad in (42, 'text', None):
        try:
            await bad
        except TypeError as e:
            print(e)
    try:
        await asyncio.gather(ok(1), 5)
    except TypeError as e:
        print(e)
    pending = ok('later')
    try:
        asyncio.run(pending)
    except RuntimeError as e:
        print(e)
    print(await pending)
    first = []

    async def record(tag, n):
        for _ in range(n):
            await asyncio.sleep(0)
        first.append(tag)
        raise RuntimeError(tag)

    try:
        await asyncio.gather(record('late', 3), record('early', 1))
    except RuntimeError as e:
        print('first failure wins:', e, first)
    return 'ok'


print(asyncio.run(main()))
try:
    asyncio.run(fail('top'))
except ValueError as e:
    print('run raised', e)
try:
    asyncio.run(5)
except TypeError as e:
    print(e)
