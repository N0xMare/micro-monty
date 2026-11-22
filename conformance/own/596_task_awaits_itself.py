# A task that awaits itself: RuntimeError inside the task at its next step.
import asyncio

ts = {}


async def other(tag, n):
    for i in range(n):
        print(tag, 'other', i)
        await asyncio.sleep(0)


async def me(tag):
    print(tag, 'start')
    try:
        await ts[tag]
    except RuntimeError as e:
        print(tag, type(e).__name__, str(e)[:50].split(' coro=')[0])
    return tag


async def after_cancel(tag):
    try:
        await asyncio.sleep(1000)
    except asyncio.CancelledError:
        print(tag, 'cancelled', asyncio.current_task().cancelling())
    try:
        await ts[tag]
    except RuntimeError as e:
        print(tag, str(e)[:50].split(' name=')[0])
    return tag


async def cancelled_first(tag):
    # A cancel requested while it runs wins over the RuntimeError.
    asyncio.current_task().cancel('mine')
    try:
        await ts[tag]
    except BaseException as e:
        print(tag, type(e).__name__, e.args)
    return tag


async def uncaught(tag):
    await ts[tag]


async def main():
    ts['a'] = asyncio.create_task(me('a'))
    await other('a', 3)
    print('a', ts['a'].done(), ts['a'].result())

    ts['named'] = asyncio.create_task(me('named'), name="it's")
    await ts['named']

    ts['c'] = asyncio.create_task(after_cancel('c'))
    await asyncio.sleep(0)
    ts['c'].cancel()
    print(await ts['c'])

    ts['d'] = asyncio.create_task(cancelled_first('d'))
    print(await ts['d'])

    # Cancelled from outside in the iteration between the await and the raise.
    ts['e'] = asyncio.create_task(me('e'))
    await asyncio.sleep(0)
    print('e cancel', ts['e'].cancel('late'))
    # (Not `await ts['e']`: after a task awaited itself, CPython's C task
    # raises "await wasn't used with future" at any later await of it while
    # it is pending; micro-monty does not.)
    await asyncio.gather(ts['e'], return_exceptions=True)
    try:
        ts['e'].result()
    except asyncio.CancelledError as e:
        print('e', 'CancelledError', e.args)

    ts['f'] = asyncio.create_task(uncaught('f'))
    try:
        await ts['f']
    except RuntimeError as e:
        print('f', str(e)[:28], ts['f'].done())

    # The main task awaits itself.
    ts['m'] = asyncio.current_task()
    try:
        await ts['m']
    except RuntimeError as e:
        print('m', str(e)[:28])

    # Awaiting itself as the main coroutine ends: it raises in the loop's
    # last iteration, before the run cancels what is left.
    ts['z'] = asyncio.create_task(me('z'))
    await asyncio.sleep(0)


try:
    asyncio.run(main())
except BaseException as e:
    print('run raised', type(e).__name__, e.args)
print('end')
