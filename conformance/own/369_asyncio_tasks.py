# asyncio.create_task, Task done/result/exception/cancel/cancelled, cancellation
# raising at the await (except/finally run), InvalidStateError, wait_for (timeout,
# None, 0, a task), as_completed (iterator, async iterator, timeout) and
# Semaphore/BoundedSemaphore.
import asyncio

S = 0.05  # CPython sleeps in real time; mm's time is virtual

log = []

async def work(name, delay, fail=False):
    log.append(('start', name))
    try:
        await asyncio.sleep(delay * S)
        if fail:
            raise ValueError(name)
        log.append(('end', name))
        return name.upper()
    except asyncio.CancelledError:
        log.append(('cancelled', name))
        raise
    finally:
        log.append(('finally', name))

async def main():
    # create_task: scheduled, runs when main yields
    t = asyncio.create_task(work('a', 0.2))
    print(type(t).__name__, t.done())
    log.append('before await')
    await asyncio.sleep(0)
    print(await t, t.done(), t.result(), t.exception(), t.cancelled())
    # cancel a running task
    t2 = asyncio.create_task(work('b', 5))
    await asyncio.sleep(0.1 * S)
    print(t2.cancel(), t2.cancel('again') if False else '')
    try:
        await t2
    except asyncio.CancelledError as e:
        print('caught cancel', repr(e), t2.cancelled(), t2.done())
    try:
        t2.result()
    except asyncio.CancelledError:
        print('result raises CancelledError')
    # cancel before start
    t3 = asyncio.create_task(work('c', 1))
    print(t3.cancel('stop'))
    try:
        await t3
    except asyncio.CancelledError as e:
        print('t3', e.args, t3.cancelled())
    # pending result
    t4 = asyncio.create_task(work('d', 1, fail=True))
    try:
        t4.result()
    except asyncio.InvalidStateError as e:
        print(type(e).__name__, e)
    try:
        t4.exception()
    except asyncio.InvalidStateError as e:
        print(type(e).__name__, e)
    try:
        await t4
    except ValueError as e:
        print('t4 failed', e, repr(t4.exception()))
    print(t4.cancel())
    # wait_for
    print(await asyncio.wait_for(work('e', 1), timeout=2 * S))
    try:
        await asyncio.wait_for(work('f', 3), timeout=1 * S)
    except TimeoutError as e:
        print('timeout', type(e).__name__, type(e.__cause__).__name__)
    print(await asyncio.wait_for(work('g', 1), None))
    try:
        await asyncio.wait_for(work('h', 1), 0)
    except TimeoutError:
        print('timeout 0')
    t5 = asyncio.create_task(work('i', 0))
    await asyncio.sleep(0.5 * S)
    print(await asyncio.wait_for(t5, 0))
    # as_completed
    coros = [work('x', 3)]
    for c in asyncio.as_completed(coros):
        print('as_completed', await c)
    ts = [asyncio.create_task(work(n, d)) for n, d in [('p', 2), ('q', 1), ('r', 1.5)]]
    async for t in asyncio.as_completed(ts):
        print('async as_completed', t.result())
    slow = asyncio.create_task(work('slow', 4))
    try:
        for c in asyncio.as_completed([work('s', 1), slow], timeout=2 * S):
            print('got', await c)
    except TimeoutError:
        print('as_completed timeout')
    print(await slow)
    try:
        asyncio.as_completed(slow)
    except TypeError as e:
        print(e)
    try:
        asyncio.create_task(5)
    except TypeError as e:
        print(e)
    # Semaphore
    sem = asyncio.Semaphore(2)
    order = []
    async def limited(i):
        async with sem:
            order.append(('in', i, sem._value))
            await asyncio.sleep(1 * S)
            order.append(('out', i))
        return i
    print(await asyncio.gather(*(limited(i) for i in range(5))))
    print(order)
    print(sem.locked(), repr(sem).split(' object')[0], repr(sem).split(' [')[1])
    b = asyncio.BoundedSemaphore(1)
    await b.acquire(); b.release()
    try:
        b.release()
    except ValueError as e:
        print(e)
    try:
        asyncio.Semaphore(-1)
    except ValueError as e:
        print(e)
    try:
        asyncio.Semaphore(1, 2)
    except TypeError as e:
        print(e)
    print(asyncio.Semaphore(value=3)._value)
    print(issubclass(asyncio.CancelledError, BaseException), issubclass(asyncio.CancelledError, Exception))
    print(asyncio.CancelledError.__module__, asyncio.TimeoutError is TimeoutError)

asyncio.run(main())
for x in log: print(x)
