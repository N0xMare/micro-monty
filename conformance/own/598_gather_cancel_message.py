# A gather that was cancelled raises the CancelledError of its last listed
# child (CPython's `_make_cancelled_error` of it): for a task that swallowed
# its cancel, a new one with the message of the last cancel it had to raise
# itself.
import asyncio
async def child(f):
    await f
async def swallow(f):
    try:
        await f
    except asyncio.CancelledError:
        return 'swallowed'
async def last_child():
    loop = asyncio.get_running_loop()
    for kinds in ('c', 's', 'cs', 'sc'):
        for rex in (True, False):
            fs = [loop.create_future() for _ in kinds]
            ts = [asyncio.create_task((child if k == 'c' else swallow)(f)) for k, f in zip(kinds, fs)]
            g = asyncio.gather(*ts, return_exceptions=rex)
            await asyncio.sleep(0)
            for f in fs: f.cancel('f')
            for t in ts: t.cancel('t')
            g.cancel('end')
            for i in range(4):
                await asyncio.sleep(0)
            try:
                r = await g
                print(kinds, rex, 'ok', r)
            except asyncio.CancelledError as e:
                print(kinds, rex, 'await', e.args, g.cancelled())
            for f in fs: f.cancel()
asyncio.run(last_child())


# The gather cancelled with a message, awaited at once, later, from
# another task, and read through result() and exception().
async def child2(f):
    await f
async def sw(f):
    try:
        await f
    except asyncio.CancelledError:
        return 'sw'
async def main():
    loop = asyncio.get_running_loop()
    for n, mk in enumerate([child2, sw]):
        for when in ('before', 'after'):
            for rex in (False, True):
                f = loop.create_future()
                t = asyncio.create_task(mk(f))
                g = asyncio.gather(t, return_exceptions=rex)
                if when == 'after':
                    await asyncio.sleep(0)
                print(g.cancel('many'), end=' ')
                try:
                    print(mk.__name__, when, rex, 'ok', await g)
                except asyncio.CancelledError as e:
                    print(mk.__name__, when, rex, e.args, g.cancelled(), t.cancelled())
    # awaited from another task, and through wait_for and shield
    f = loop.create_future()
    g = asyncio.gather(f)
    async def waiter():
        try:
            await g
        except asyncio.CancelledError as e:
            return e.args
    w = asyncio.create_task(waiter())
    await asyncio.sleep(0)
    g.cancel('m2')
    print(await w)
    g = asyncio.gather(loop.create_future())
    g.cancel('m3')
    await asyncio.sleep(0)
    await asyncio.sleep(0)
    try:
        await g
    except asyncio.CancelledError as e:
        print('late', e.args)
    try:
        g.result()
    except asyncio.CancelledError as e:
        print('result', e.args)
    try:
        g.exception()
    except asyncio.CancelledError as e:
        print('exception', e.args)
asyncio.run(main())


# The last listed child decides, not the one that finished last: cancelled
# through the task that awaits the gather, and alone.
async def sl3(catch):
    try:
        await asyncio.sleep(30)
    except asyncio.CancelledError as e:
        print('  child got', repr(e))
        if catch:
            return 'caught'
        raise
async def listed():
    for first in (True, False):
        box = []
        async def body():
            z = asyncio.ensure_future(asyncio.sleep(0))
            b = asyncio.ensure_future(sl3(True))
            box.append(b)
            g = asyncio.gather(*([b, z] if first else [z, b]), return_exceptions=True)
            try:
                return await g
            except asyncio.CancelledError as e:
                print('  body got', repr(e), g.cancelled(), g.done(), repr(g.exception()), e is g.exception())
                raise
        t = asyncio.create_task(body())
        for _ in range(2):
            await asyncio.sleep(0)
        t.cancel('G')
        print(await asyncio.gather(t, return_exceptions=True))
        # the gather alone, not awaited by a task
        z = asyncio.ensure_future(asyncio.sleep(0))
        b = asyncio.ensure_future(sl3(True))
        g = asyncio.gather(*([b, z] if first else [z, b]), return_exceptions=True)
        await asyncio.sleep(0)
        await asyncio.sleep(0)
        g.cancel('H')
        await asyncio.sleep(0)
        await asyncio.sleep(0)
        await asyncio.sleep(0)
        print(g.done(), repr(g.exception()) if not g.cancelled() else 'cancelled')
asyncio.run(listed())
