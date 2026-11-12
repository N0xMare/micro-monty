# asyncio cancellation as CPython 3.14 does it: cancel() reaches what a task
# awaits (another task, a gather's children, a future, an __await__
# iterator); cancel() on the running task and on a task that never ran;
# cancelling()/uncancel(); messages and their retrieval; gather with and
# without return_exceptions; wait_for, as_completed and Semaphore under
# cancellation; and asyncio.run cancelling the tasks left when main ends.
import asyncio

log = print
LONG = 30  # never elapses: always cancelled first


async def sleeper(name, catch=False):
    try:
        await asyncio.sleep(LONG)
        return name
    except asyncio.CancelledError as e:
        log(name, 'cancelled', repr(e))
        if catch:
            return 'caught ' + name
        raise
    finally:
        log(name, 'finally')


async def awaiter(t):
    try:
        return await t
    except asyncio.CancelledError as e:
        log('awaiter got', repr(e), t.done(), t.cancelled())
        raise


async def task_chain():
    log('-- awaited task')
    inner = asyncio.create_task(sleeper('inner'))
    outer = asyncio.create_task(awaiter(inner))
    await asyncio.sleep(0)
    log('cancel', outer.cancel('stop'), outer.cancelling(), inner.cancelling())
    r = await asyncio.gather(outer, return_exceptions=True)
    log(r, outer.cancelled(), inner.cancelled())
    for t in (inner, outer):
        for _ in range(2):
            try:
                t.result()
            except asyncio.CancelledError as e:
                log('result', repr(e))
    log('-- awaited task catches')
    inner = asyncio.create_task(sleeper('inner2', catch=True))
    outer = asyncio.create_task(awaiter(inner))
    await asyncio.sleep(0)
    log('cancel', outer.cancel('m'))
    log(await outer, outer.cancelled(), outer.cancelling(), inner.cancelled())
    log('-- chain of three')
    a = asyncio.create_task(sleeper('a'))
    b = asyncio.create_task(awaiter(a))
    c = asyncio.create_task(awaiter(b))
    await asyncio.sleep(0)
    c.cancel()
    log(await asyncio.gather(a, b, c, return_exceptions=True))
    log([t.cancelling() for t in (a, b, c)])


async def gathering():
    for keep in (False, True):
        log('-- gather in a task, return_exceptions', keep)

        async def body():
            try:
                r = await asyncio.gather(sleeper('g1'), sleeper('g2', catch=True), asyncio.sleep(0),
                                         return_exceptions=keep)
                log('gather returned', r)
            except asyncio.CancelledError as e:
                log('body got', repr(e))
                raise
        t = asyncio.create_task(body())
        await asyncio.sleep(0)
        await asyncio.sleep(0)
        log('cancel', t.cancel('G'))
        log(await asyncio.gather(t, return_exceptions=True), t.cancelled())
    log('-- gather.cancel()')
    g = asyncio.gather(sleeper('h1'), sleeper('h2'))
    await asyncio.sleep(0)
    log(g.cancel('H'), g.done())
    try:
        await g
    except asyncio.CancelledError as e:
        log('g', repr(e), g.done(), g.cancelled())
    log('-- gather cancelled before its children start')
    g = asyncio.gather(sleeper('k1'), sleeper('k2'))
    log(g.cancel(), g.cancel())
    try:
        await g
    except asyncio.CancelledError as e:
        log('g', repr(e), g.cancelled())
    log('-- nested gathers')
    t = asyncio.create_task(awaiter(asyncio.gather(asyncio.gather(sleeper('n1')), sleeper('n2'))))
    await asyncio.sleep(0)
    await asyncio.sleep(0)
    t.cancel('N')
    log(await asyncio.gather(t, return_exceptions=True))
    log('-- cancelled children with return_exceptions')
    x = asyncio.create_task(sleeper('x'))
    y = asyncio.create_task(sleeper('y'))
    await asyncio.sleep(0)
    x.cancel('X')
    y.cancel()
    log(await asyncio.gather(x, y, return_exceptions=True))


class Steps:
    def __await__(self):
        try:
            yield
            yield
            log('steps resumed')
        except asyncio.CancelledError as e:
            log('steps got', repr(e))
            raise
        return 'done'


async def iterators():
    log('-- __await__ iterator')

    async def body():
        try:
            return await Steps()
        except asyncio.CancelledError as e:
            log('body got', repr(e))
            raise
    t = asyncio.create_task(body())
    await asyncio.sleep(0)
    log('cancel', t.cancel('I'))
    log(await asyncio.gather(t, return_exceptions=True))


async def self_cancel():
    log('-- the running task cancels itself')
    me = []

    async def body(msg):
        t = me[-1]
        log('cancel', t.cancel(msg), t.cancelling(), t.cancelled(), t.done())
        log('cancel again', t.cancel(msg), t.cancelling())
        try:
            await asyncio.sleep(0)
        except asyncio.CancelledError as e:
            log('got', repr(e))
            log('uncancel', t.uncancel(), t.uncancel(), t.uncancel())
        return 'went on'
    for msg in (None, 'own'):
        me.append(asyncio.create_task(body(msg)))
        log(await me[-1], me[-1].cancelled())
    log('-- cancel, then return')

    async def returns():
        me[-1].cancel('late')
        return 'lost'
    me.append(asyncio.create_task(returns()))
    log(await asyncio.gather(me[-1], return_exceptions=True), me[-1].cancelled())
    for _ in range(2):
        try:
            me[-1].result()
        except asyncio.CancelledError as e:
            log('result', repr(e))
    log('-- uncancel drops a pending cancel')

    async def keeps():
        me[-1].cancel()
        me[-1].uncancel()
        await asyncio.sleep(0)
        return 'kept'
    me.append(asyncio.create_task(keeps()))
    log(await me[-1])


async def never():
    log('never body runs')


async def unstarted():
    log('-- a task that never ran')
    t = asyncio.create_task(never())
    log(t.cancel('first'), t.done(), t.cancelled(), t.cancelling())
    log(t.cancel(), t.cancelling())
    await asyncio.sleep(0)
    log(t.done(), t.cancelled())
    for _ in range(2):
        try:
            t.result()
        except asyncio.CancelledError as e:
            log('result', repr(e))
    t = asyncio.create_task(never())
    t.cancel('only')
    try:
        await t
    except asyncio.CancelledError as e:
        log('await', repr(e))
    try:
        t.exception()
    except asyncio.CancelledError as e:
        log('exception', repr(e))
    log(t.cancel())


async def timeouts():
    log('-- wait_for cancels the task it waits on')
    inner = asyncio.create_task(sleeper('wf'))
    try:
        await asyncio.wait_for(inner, 0.01)
    except TimeoutError as e:
        log('timeout', repr(e), repr(e.__cause__), inner.cancelled())
    log('-- the awaitable catches the timeout')
    log(await asyncio.wait_for(sleeper('wf2', catch=True), 0.01))
    log('-- an outer cancel during wait_for stays a cancel')
    me = []

    async def runner():
        try:
            await asyncio.wait_for(sleeper('wf3'), LONG)
        except TimeoutError:
            log('timeout (wrong)')
        except asyncio.CancelledError as e:
            log('runner got', repr(e), me[0].cancelling())
            raise
    me.append(asyncio.create_task(runner()))
    await asyncio.sleep(0)
    me[0].cancel('outer')
    log(await asyncio.gather(me[0], return_exceptions=True))
    log('-- wait_for(timeout=0)')
    t = asyncio.create_task(sleeper('wf0'))
    await asyncio.sleep(0)
    try:
        await asyncio.wait_for(t, 0)
    except TimeoutError as e:
        log('timeout0', repr(e.__cause__), t.cancelled())


async def quick():
    await asyncio.sleep(0)
    return 'quick'


async def completed():
    log('-- as_completed: the consumer is cancelled, not the awaitables')
    slow = asyncio.create_task(sleeper('ac slow'))

    async def consume():
        try:
            for f in asyncio.as_completed([slow, quick()]):
                log('got', await f)
        except asyncio.CancelledError as e:
            log('consumer got', repr(e))
            raise
    t = asyncio.create_task(consume())
    for _ in range(4):
        await asyncio.sleep(0)
    t.cancel('AC')
    log(await asyncio.gather(t, return_exceptions=True), slow.done())
    slow.cancel()
    log(await asyncio.gather(slow, return_exceptions=True))


async def locks():
    log('-- Semaphore waiters')
    sem = asyncio.Semaphore(1)

    async def holder(n):
        try:
            async with sem:
                log('hold', n)
                await asyncio.sleep(0)
                log('release', n)
        except asyncio.CancelledError:
            log('holder cancelled', n)
            raise
    ts = [asyncio.create_task(holder(n)) for n in range(3)]
    await asyncio.sleep(0)
    log(sem.locked(), ts[1].cancel())
    log(await asyncio.gather(*ts, return_exceptions=True), sem.locked())
    log('-- a woken waiter is cancelled before it runs')
    sem = asyncio.Semaphore(0)
    t = asyncio.create_task(holder(9))
    await asyncio.sleep(0)
    sem.release()
    log(t.cancel())
    log(await asyncio.gather(t, return_exceptions=True), sem.locked())
    log('-- futures')
    f = asyncio.Future()
    log(f.cancel('F'), f.cancelled(), f.cancel(), f.done())
    for _ in range(2):
        try:
            await f
        except asyncio.CancelledError as e:
            log('await', repr(e))
    f = asyncio.Future()

    async def on_future():
        try:
            await f
        except asyncio.CancelledError as e:
            log('on_future got', repr(e))
            raise
    t = asyncio.create_task(on_future())
    await asyncio.sleep(0)
    t.cancel('W')
    log(f.cancelled(), t.done())
    await asyncio.sleep(0)
    log(t.cancelled())


async def main():
    await task_chain()
    await gathering()
    await iterators()
    await self_cancel()
    await unstarted()
    await timeouts()
    await completed()
    await locks()
    return 'main result'


log(asyncio.run(main()))


# asyncio.run cancels the tasks left when main ends; they run until their
# cancellation is done. A task ready when main ends runs one more step.
async def background(name, steps):
    log(name, 'starts')
    try:
        for i in range(steps):
            await asyncio.sleep(0)
            log(name, 'step', i)
        await asyncio.sleep(LONG)
    except asyncio.CancelledError as e:
        log(name, 'cancelled', repr(e))
        await asyncio.sleep(0)
        log(name, 'cleaned up')
        raise
    finally:
        log(name, 'finally')


async def returns():
    asyncio.create_task(background('bg1', 2))
    await asyncio.sleep(0)
    log('main returns')
    return 1


async def spawns():
    asyncio.create_task(background('late', 0))
    log('spawner done')


async def raises():
    asyncio.create_task(spawns())
    log('main raises')
    raise ValueError('boom')


async def no_start():
    asyncio.create_task(never())
    log('main ends at once')


handles = []


async def keeps_handle():
    handles.append(asyncio.create_task(background('kept', 0)))
    await asyncio.sleep(0)


log(asyncio.run(returns()))
try:
    asyncio.run(raises())
except ValueError as e:
    log('caught', e)
asyncio.run(no_start())
asyncio.run(keeps_handle())
log(handles[0].done(), handles[0].cancelled(), handles[0].cancelling())
