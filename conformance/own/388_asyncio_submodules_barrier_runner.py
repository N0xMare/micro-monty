# asyncio's submodules (the same objects as asyncio's), Barrier (wait,
# abort, reset, a cancelled waiter, the context manager), Runner (runs in
# one copied context, task numbering, closed state, nested-run errors) and
# new_event_loop/set_event_loop with loop.run_until_complete on a
# coroutine, a gather, a task made before the run and wait_for.
import asyncio, contextvars
from asyncio import locks, taskgroups, tasks, queues, timeouts, exceptions, events, futures
import asyncio.runners
print(locks.Barrier is asyncio.Barrier, taskgroups.TaskGroup is asyncio.TaskGroup, tasks.gather is asyncio.gather)
print(exceptions.CancelledError is asyncio.CancelledError, events.get_running_loop is asyncio.get_running_loop)
print(asyncio.runners.Runner is asyncio.Runner, asyncio.locks is locks, queues.Queue, timeouts.timeout is asyncio.timeout, futures.Future is asyncio.Future)
print(asyncio.BrokenBarrierError.__mro__, asyncio.Barrier.__module__, asyncio.Runner.__module__)


def r(b):
    return repr(b).split(' [')[1]


async def worker(b, i, log):
    idx = await b.wait()
    log.append((i, idx))
    return idx


async def barrier():
    b = asyncio.Barrier(3)
    print(r(b), b.parties, b.n_waiting, b.broken)
    log = []
    ts = [asyncio.create_task(worker(b, i, log)) for i in range(3)]
    await asyncio.sleep(0)
    print(r(b), b.n_waiting)
    print(await asyncio.gather(*ts), log)
    t = asyncio.create_task(b.wait())
    await asyncio.sleep(0)
    print(b.n_waiting)
    await b.abort()
    try:
        await t
    except asyncio.BrokenBarrierError as e:
        print('broken', repr(e))
    print(r(b), b.broken)
    try:
        await b.wait()
    except asyncio.BrokenBarrierError as e:
        print('broken2', e)
    await b.reset()
    print(r(b))
    t1 = asyncio.create_task(b.wait())
    t2 = asyncio.create_task(b.wait())
    await asyncio.sleep(0)
    await b.reset()
    for t in (t1, t2):
        try:
            await t
        except asyncio.BrokenBarrierError as e:
            print('reset', e)
    print(r(b))
    t1 = asyncio.create_task(b.wait())
    await asyncio.sleep(0)
    t1.cancel()
    try:
        await t1
    except asyncio.CancelledError:
        print('cancelled', b.n_waiting, r(b))
    async with asyncio.Barrier(1) as i:
        print('ctx', i)
    try:
        asyncio.Barrier(0)
    except ValueError as e:
        print(e)
asyncio.run(barrier())

v = contextvars.ContextVar('v', default=0)


async def setv(x):
    old = v.get()
    v.set(x)
    return old


async def main(n):
    loop = asyncio.get_running_loop()
    print('main', n, loop.is_running(), asyncio.current_task().get_name())
    return n * 2


with asyncio.Runner() as rn:
    loop = rn.get_loop()
    print(type(loop).__name__, loop.is_running(), loop.is_closed(), rn.get_loop() is loop)
    print(asyncio.get_event_loop() is loop)
    print(rn.run(main(1)), rn.run(main(2)))
    print(rn.run(setv(5)), rn.run(setv(6)), v.get())
    ctx = contextvars.copy_context()
    print(rn.run(setv(7), context=ctx), ctx[v] if v in ctx else None, v.get())

    async def chk():
        return asyncio.get_running_loop() is loop
    print('same loop', rn.run(chk()))
    try:
        rn.run(42)
    except TypeError as e:
        print('TE', e)
print(loop.is_closed())
c = main(3)
try:
    rn.run(c)
except RuntimeError as e:
    print('RE', e)
c.close()
rn.close()
try:
    asyncio.get_event_loop()
except RuntimeError as e:
    print('no loop', e)
loop = asyncio.new_event_loop()
asyncio.set_event_loop(loop)
print(asyncio.get_event_loop() is loop, loop.is_running())
print(loop.run_until_complete(main(4)))


async def a(x):
    await asyncio.sleep(0.01)
    return x
print(loop.run_until_complete(asyncio.gather(a(1), a(2))))
t = loop.create_task(a(3))
print(loop.run_until_complete(t), t.get_name())
print(loop.run_until_complete(asyncio.wait_for(a(4), 1)))


async def ruc():
    c = main(5)
    try:
        loop.run_until_complete(c)
    except RuntimeError as e:
        print('nested', e)
    c.close()
    c = main(6)
    try:
        asyncio.Runner().run(c)
    except RuntimeError as e:
        print('nested2', e)
    c.close()
loop.run_until_complete(ruc())
loop.close()
loop.close()
print(loop.is_closed())
c = main(7)
try:
    loop.run_until_complete(c)
except RuntimeError as e:
    print('closed', e)
c.close()
try:
    asyncio.set_event_loop(3)
except TypeError as e:
    print(e)
asyncio.set_event_loop(None)
for bad in [lambda: asyncio.Runner(foo=1), lambda: asyncio.Runner(1)]:
    try:
        bad()
    except TypeError as e:
        print(e)
