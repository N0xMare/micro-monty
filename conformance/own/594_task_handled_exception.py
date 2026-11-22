# The handled exception a task or a loop callback starts with: CPython
# runs each task step and callback from the loop, beneath which the
# handler `asyncio.run()` was called in is still open. So inside a run made
# within `except E`, `sys.exception()` is E in every task and callback,
# also one created later, from a callback, or while its creator handles
# something else (which it does not see); a run made with no handler open
# gives None.
import asyncio, sys

def see(tag):
    out = [tag, repr(sys.exception())]
    try:
        raise
    except RuntimeError as e:
        out.append('none' if 'No active' in str(e) else 're ' + repr(e))
    except BaseException as e:
        out.append('re ' + repr(e))
    try:
        raise OSError('probe')
    except OSError as e:
        out.append(repr(e.__context__))
    print(*out)

async def worker(tag, own=False):
    see(tag + ' start')
    await asyncio.sleep(0)
    see(tag + ' after sleep')
    if own:
        try:
            raise KeyError(tag)
        except KeyError:
            await asyncio.sleep(0)
            see(tag + ' in own handler')
        see(tag + ' after own handler')

def cb(tag, *a):
    see('callback ' + tag)
    if tag == 'spawns':
        asyncio.get_running_loop().create_task(worker('from callback'))

async def main():
    loop = asyncio.get_running_loop()
    see('main start')
    t1 = asyncio.create_task(worker('early'))
    await asyncio.sleep(0)
    try:
        raise ValueError('main handler')
    except ValueError:
        t2 = asyncio.create_task(worker('made in handler', own=True))
        loop.call_soon(cb, 'soon in handler')
        fut = loop.create_future()
        fut.add_done_callback(lambda f: cb('done cb'))
        fut.set_result(1)
        await asyncio.sleep(0)
        see('main in handler')
        await asyncio.gather(worker('gathered in handler'), t2)
    loop.call_soon(cb, 'spawns')
    loop.call_later(0.01, cb, 'timer')
    await asyncio.sleep(0.02)
    async with asyncio.TaskGroup() as tg:
        tg.create_task(worker('group'))
    t3 = asyncio.create_task(worker('late'))
    await t1
    await t3
    await asyncio.sleep(0)
    see('main end')
    t4 = asyncio.create_task(worker('left over'))

print('--- run inside a handler')
try:
    raise LookupError('run caller')
except LookupError:
    asyncio.run(main())
    see('after run')
print('--- run with no handler')
asyncio.run(main())
print('--- second run inside another handler')
try:
    raise TypeError('second caller')
except TypeError:
    asyncio.run(worker('main coroutine itself', own=True))
print('--- a Runner inside a handler')
with asyncio.Runner() as r:
    try:
        raise ArithmeticError('runner caller')
    except ArithmeticError:
        async def spawn():
            await asyncio.create_task(worker('runner task'))

        r.run(spawn())
    r.run(worker('runner, no handler'))
see('end')
