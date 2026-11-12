# The second asyncio batch as CPython 3.14 does it: Lock, Event, Condition,
# Semaphore/BoundedSemaphore and the queues (FIFO waiters, cancellation
# while waiting and right after being woken, shutdown); ExceptionGroup;
# TaskGroup, timeout/timeout_at with reschedule, shield and wait; the
# running loop's call_soon/call_later/call_at, create_future and done
# callbacks; task names; and asyncio.run cancelling what is left. (A
# TaskGroup cancels its other tasks in creation order; CPython walks a set,
# so here one task is left to cancel.)
eg = ExceptionGroup('m', [ValueError(1), KeyError('k'), ExceptionGroup('n', [TypeError('t')])])
print(type(BaseExceptionGroup('x', [ValueError()])).__name__, type(BaseExceptionGroup('x', [KeyboardInterrupt()])).__name__)
print(repr(eg), str(eg), eg.message, eg.exceptions)
a, b = eg.split(TypeError)
print(repr(a), repr(b))
print(repr(eg.subgroup(lambda e: isinstance(e, ValueError))))
print(isinstance(eg, Exception), issubclass(ExceptionGroup, BaseExceptionGroup), ExceptionGroup.__module__)
for bad in [(1, [ValueError()]), ('m', []), ('m', [1]), ('m', {ValueError()})]:
    try: ExceptionGroup(*bad)
    except Exception as e: print(type(e).__name__, e)
try: ExceptionGroup('m', [KeyboardInterrupt()])
except TypeError as e: print(e)
try:
    raise eg
except ExceptionGroup as e:
    print('caught', e.exceptions[0])

import asyncio
log = print
S = 0.01

async def locks():
    log('-- Lock FIFO and cancel')
    lock = asyncio.Lock()
    order = []
    async def w(n, hold):
        try:
            async with lock:
                order.append(n)
                await asyncio.sleep(hold)
        except asyncio.CancelledError:
            log('w cancelled', n, lock.locked())
            raise
    ts = [asyncio.create_task(w(i, S)) for i in range(4)]
    await asyncio.sleep(0)
    log(lock.locked(), len(lock._waiters))
    ts[2].cancel()
    await asyncio.gather(*ts, return_exceptions=True)
    log(order, lock.locked())
    try:
        lock.release()
    except RuntimeError as e:
        log(e)
    log('-- woken then cancelled')
    await lock.acquire()
    t1 = asyncio.create_task(lock.acquire())
    t2 = asyncio.create_task(lock.acquire())
    await asyncio.sleep(0)
    lock.release()
    t1.cancel()
    r = await asyncio.gather(t1, t2, return_exceptions=True)
    log(r, lock.locked())
    lock.release()
    log('-- Event')
    ev = asyncio.Event()
    async def waiter(n):
        await ev.wait()
        log('woke', n)
    ts = [asyncio.create_task(waiter(i)) for i in range(3)]
    await asyncio.sleep(0)
    log(ev.is_set(), len(ev._waiters))
    ts[1].cancel()
    ev.set()
    log(await asyncio.gather(*ts, return_exceptions=True))
    ev.clear()
    log(ev.is_set(), await asyncio.wait_for(asyncio.shield(ev.wait()), 0.01) if False else 'skip')
    log('-- Condition')
    cond = asyncio.Condition()
    items = []
    async def consumer(n):
        async with cond:
            await cond.wait_for(lambda: items)
            log('consumer', n, items.pop(0))
    cs = [asyncio.create_task(consumer(i)) for i in range(3)]
    await asyncio.sleep(0)
    async with cond:
        items.extend([10, 20])
        cond.notify(2)
    await asyncio.sleep(0)
    async with cond:
        items.append(30)
        cond.notify_all()
    await asyncio.gather(*cs)
    try:
        cond.notify()
    except RuntimeError as e:
        log(e)
    try:
        await cond.wait()
    except RuntimeError as e:
        log(e)
    log('-- Condition wait cancelled')
    async def cw():
        async with cond:
            try:
                await cond.wait()
            except asyncio.CancelledError:
                log('cw cancelled, locked', cond.locked())
                raise
    t = asyncio.create_task(cw())
    await asyncio.sleep(0)
    t.cancel()
    log(await asyncio.gather(t, return_exceptions=True), cond.locked())
    log('-- BoundedSemaphore')
    bs = asyncio.BoundedSemaphore(2)
    await bs.acquire()
    bs.release()
    try:
        bs.release()
    except ValueError as e:
        log(e)

async def queues():
    log('-- Queue')
    q = asyncio.Queue(2)
    log(q.maxsize, q.empty(), q.full(), str(q))
    async def producer():
        for i in range(5):
            await q.put(i)
            log('put', i)
        await q.join()
        log('joined')
    async def consumer():
        while True:
            x = await q.get()
            log('got', x)
            q.task_done()
            if x == 4:
                return
    await asyncio.gather(producer(), consumer())
    try:
        q.get_nowait()
    except asyncio.QueueEmpty as e:
        log('empty', repr(e))
    q.put_nowait(1); q.put_nowait(2)
    try:
        q.put_nowait(3)
    except asyncio.QueueFull as e:
        log('full', repr(e))
    log(str(q))
    log('-- getters cancelled')
    q = asyncio.Queue()
    gs = [asyncio.create_task(q.get()) for _ in range(3)]
    await asyncio.sleep(0)
    gs[0].cancel()
    q.put_nowait('a'); q.put_nowait('b')
    log(await asyncio.gather(*gs, return_exceptions=True))
    log('-- Lifo and Priority')
    lq = asyncio.LifoQueue(); pq = asyncio.PriorityQueue()
    for x in (3, 1, 2):
        lq.put_nowait(x); pq.put_nowait(x)
    log([lq.get_nowait() for _ in range(3)], [pq.get_nowait() for _ in range(3)])
    log('-- shutdown')
    q = asyncio.Queue(1)
    q.put_nowait('x')
    async def blocked_put():
        try:
            await q.put('y')
        except asyncio.QueueShutDown as e:
            log('put shut', repr(e))
    async def blocked_get(q2):
        try:
            await q2.get()
        except asyncio.QueueShutDown:
            log('get shut')
    p = asyncio.create_task(blocked_put())
    q2 = asyncio.Queue()
    g = asyncio.create_task(blocked_get(q2))
    await asyncio.sleep(0)
    q.shutdown()
    q2.shutdown(immediate=True)
    await asyncio.gather(p, g)
    log(q.get_nowait(), str(q))
    try:
        q.get_nowait()
    except asyncio.QueueShutDown:
        log('drained')
    try:
        q.task_done(); q.task_done()
    except ValueError as e:
        log(e)

async def main():
    await locks()
    await queues()

asyncio.run(main())

LONG = 30

async def child(name, d, fail=None):
    try:
        await asyncio.sleep(d)
        if fail:
            raise fail
        log(name, 'done')
        return name
    except asyncio.CancelledError:
        log(name, 'cancelled')
        raise
    finally:
        log(name, 'finally')

async def taskgroups():
    log('-- TaskGroup ok')
    async with asyncio.TaskGroup() as tg:
        a = tg.create_task(child('a', 0.01))
        b = tg.create_task(child('b', 0.02), name='bee')
        log(repr(tg), b.get_name())
    log(a.result(), b.result(), repr(tg))
    log('-- TaskGroup error cancels siblings')
    try:
        async with asyncio.TaskGroup() as tg:
            tg.create_task(child('c', LONG))
            tg.create_task(child('d', 0.01, ValueError('d failed')))
            await asyncio.sleep(LONG)
            log('not reached')
    except ExceptionGroup as eg:
        log('eg', repr(eg), [repr(e) for e in eg.exceptions])
        log(eg.split(ValueError)[0].exceptions)
    log('-- body raises')
    try:
        async with asyncio.TaskGroup() as tg:
            tg.create_task(child('f', LONG))
            await asyncio.sleep(0)
            raise RuntimeError('body')
    except BaseExceptionGroup as eg:
        log('eg2', type(eg).__name__, eg.exceptions)
    log('-- outer cancel through a TaskGroup')
    async def runner():
        async with asyncio.TaskGroup() as tg:
            tg.create_task(child('g', LONG))
            await asyncio.sleep(LONG)
    t = asyncio.create_task(runner())
    await asyncio.sleep(0)
    await asyncio.sleep(0)
    t.cancel('out')
    log(await asyncio.gather(t, return_exceptions=True), t.cancelled())
    log('-- create_task after exit')
    tg = asyncio.TaskGroup()
    async with tg:
        pass
    c = child('h', 0)
    try:
        tg.create_task(c)
    except RuntimeError as e:
        log(str(e).replace(repr(tg), 'TG'))

async def timeouts():
    log('-- timeout')
    try:
        async with asyncio.timeout(0.01) as cm:
            log(repr(cm).split(' when')[0], cm.expired())
            await child('t1', LONG)
    except TimeoutError as e:
        log('timeout', repr(e), type(e.__cause__).__name__, cm.expired(), repr(cm))
    async with asyncio.timeout(1) as cm:
        await asyncio.sleep(0.01)
    log(cm.expired(), repr(cm))
    log('-- reschedule')
    try:
        async with asyncio.timeout(None) as cm:
            log(cm.when())
            cm.reschedule(asyncio.get_running_loop().time() + 0.01)
            await asyncio.sleep(LONG)
    except TimeoutError:
        log('rescheduled timeout fired')
    try:
        cm.reschedule(1)
    except RuntimeError as e:
        log(e)
    try:
        asyncio.timeout(1).reschedule(1)
    except RuntimeError as e:
        log(e)
    log('-- timeout_at, inner catches')
    async with asyncio.timeout_at(asyncio.get_running_loop().time() + 0.01):
        try:
            await asyncio.sleep(LONG)
        except asyncio.CancelledError:
            log('inner swallowed')
    log('after swallowed')
    log('-- outer cancel inside timeout')
    async def r():
        async with asyncio.timeout(LONG):
            await asyncio.sleep(LONG)
    t = asyncio.create_task(r())
    await asyncio.sleep(0)
    t.cancel()
    log(await asyncio.gather(t, return_exceptions=True))

async def shields():
    log('-- shield')
    inner = asyncio.create_task(child('sh', 0.02))
    outer = asyncio.shield(inner)
    async def waiter():
        return await outer
    w = asyncio.create_task(waiter())
    await asyncio.sleep(0)
    w.cancel()
    log(await asyncio.gather(w, return_exceptions=True), outer.cancelled(), inner.done())
    log(await inner)
    try:
        await asyncio.wait_for(asyncio.shield(child('sh2', 0.02)), 0.01)
    except TimeoutError:
        log('shield timed out')
    await asyncio.sleep(0.02)
    s = asyncio.shield(child('sh3', 0, ValueError('v')))
    try:
        await s
    except ValueError as e:
        log('shield exc', e)

async def waits():
    log('-- wait')
    t1 = asyncio.create_task(child('w1', 0.01), name='w1')
    t2 = asyncio.create_task(child('w2', 0.03), name='w2')
    t3 = asyncio.create_task(child('w3', 0.02, ValueError('w3')), name='w3')
    done, pending = await asyncio.wait([t1, t2, t3], return_when=asyncio.FIRST_COMPLETED)
    log(sorted(t.get_name() for t in done), sorted(t.get_name() for t in pending))
    done, pending = await asyncio.wait([t1, t2, t3], return_when=asyncio.FIRST_EXCEPTION)
    log(sorted(t.get_name() for t in done), sorted(t.get_name() for t in pending))
    done, pending = await asyncio.wait([t2], timeout=0.001)
    log(len(done), len(pending))
    done, pending = await asyncio.wait({t1, t2, t3})
    log(sorted(t.get_name() for t in done), pending)
    cx = child('x', 0)
    for bad in ([], [cx], t1):
        try:
            await asyncio.wait(bad)
        except (TypeError, ValueError) as e:
            log(type(e).__name__, e)
    cx.close()
    try:
        await asyncio.wait([t1], return_when='X')
    except ValueError as e:
        log(e)

async def loops():
    log('-- loop')
    loop = asyncio.get_running_loop()
    log(loop is asyncio.get_event_loop(), loop.is_running(), type(loop.time()).__name__)
    got = []
    h1 = loop.call_soon(got.append, 1)
    h2 = loop.call_soon(got.append, 2)
    h2.cancel()
    loop.call_later(0.01, got.append, 3)
    th = loop.call_at(loop.time() + 0.005, got.append, 4)
    log(h2.cancelled(), repr(h2))
    await asyncio.sleep(0.02)
    log(got)
    f = loop.create_future()
    loop.call_soon(f.set_result, 'fr')
    log(await f)
    f = loop.create_future()
    f.set_exception(KeyError)
    try:
        await f
    except KeyError as e:
        log('set_exception', repr(e))
    f = loop.create_future()
    f.set_exception(StopIteration())
    log(repr(f.exception()), type(f.exception().__cause__).__name__)
    f = loop.create_future()
    f.set_result(1)
    try:
        f.set_result(2)
    except asyncio.InvalidStateError as e:
        log('ise', e)
    log('-- done callbacks')
    t = asyncio.create_task(child('cb', 0))
    t.add_done_callback(lambda x: log('cb1', x.result()))
    def cb2(x): log('cb2')
    t.add_done_callback(cb2)
    t.add_done_callback(cb2)
    log(t.remove_done_callback(cb2))
    await t
    await asyncio.sleep(0)
    t.add_done_callback(lambda x: log('late cb', x.done()))
    await asyncio.sleep(0)
    log('-- names and current/all tasks')
    me = asyncio.current_task()
    log(me.get_name().startswith('Task-'), me in asyncio.all_tasks())
    t = asyncio.create_task(child('n', 0), name=42)
    log(t.get_name())
    t.set_name('renamed')
    log(t.get_name(), t.get_coro() is not None, t.get_loop() is loop)
    await t

async def main():
    await taskgroups()
    await timeouts()
    await shields()
    await waits()
    await loops()

asyncio.run(main())
try:
    asyncio.get_running_loop()
except RuntimeError as e:
    log(e)


async def lock_cancel():
    log('-- lock fifo + cancel')
    lk = asyncio.Lock()
    order = []
    async def w(i):
        try:
            async with lk:
                order.append(i)
                await asyncio.sleep(0)
        except asyncio.CancelledError:
            order.append(('c', i))
            raise
    await lk.acquire()
    ts = [asyncio.create_task(w(i)) for i in range(5)]
    await asyncio.sleep(0)
    ts[1].cancel(); ts[3].cancel()
    lk.release()
    await asyncio.gather(*ts, return_exceptions=True)
    log(order, lk.locked())
    # cancel the waiter that was just woken
    await lk.acquire()
    a = asyncio.create_task(w('a')); b = asyncio.create_task(w('b'))
    await asyncio.sleep(0)
    lk.release()
    a.cancel()
    await asyncio.gather(a, b, return_exceptions=True)
    log(order, lk.locked())

async def event_cond():
    log('-- event/condition cancel')
    ev = asyncio.Event()
    async def ew(i):
        await ev.wait(); return i
    ts = [asyncio.create_task(ew(i)) for i in range(3)]
    await asyncio.sleep(0)
    ts[0].cancel()
    ev.set()
    log(await asyncio.gather(*ts, return_exceptions=True))
    c = asyncio.Condition()
    got = []
    async def cw(i):
        async with c:
            try:
                await c.wait()
                got.append(i)
            except asyncio.CancelledError:
                got.append(('c', i, c.locked()))
                raise
    ts = [asyncio.create_task(cw(i)) for i in range(4)]
    await asyncio.sleep(0)
    async with c:
        c.notify(2)
        ts[0].cancel()
    await asyncio.sleep(0); await asyncio.sleep(0)
    async with c:
        c.notify_all()
    log(await asyncio.gather(*ts, return_exceptions=True), got)
    r = await asyncio.wait_for(c.wait_for(lambda: True) if False else asyncio.sleep(0, 'x'), 1)
    log(r)

async def queue_cancel():
    log('-- queue cancel')
    q = asyncio.Queue(1)
    await q.put(0)
    async def p(i):
        await q.put(i); return i
    ts = [asyncio.create_task(p(i)) for i in range(1, 5)]
    await asyncio.sleep(0)
    ts[0].cancel()
    out = []
    for _ in range(3):
        out.append(await q.get())
    log(out, await asyncio.gather(*ts, return_exceptions=True), q.qsize())
    log(q.get_nowait())
    gs = [asyncio.create_task(q.get()) for _ in range(3)]
    await asyncio.sleep(0)
    gs[1].cancel()
    q.put_nowait('A'); q2 = q
    await asyncio.sleep(0)
    q.put_nowait('B')
    log(await asyncio.gather(*gs, return_exceptions=True))
    try:
        await asyncio.wait_for(q.get(), 0.01)
    except TimeoutError:
        log('get timed out', len(q._getters))
    q = asyncio.Queue()
    g = asyncio.create_task(q.get())
    await asyncio.sleep(0)
    q.shutdown()
    try:
        await g
    except asyncio.QueueShutDown as e:
        log('shutdown', repr(e))
    q = asyncio.Queue(); q.put_nowait(1); q.put_nowait(2)
    j = asyncio.create_task(q.join())
    await asyncio.sleep(0)
    q.shutdown(immediate=True)
    log(await j, q.empty())

async def semas():
    log('-- semaphore')
    s = asyncio.BoundedSemaphore(1)
    await s.acquire()
    ts = [asyncio.create_task(s.acquire()) for _ in range(3)]
    await asyncio.sleep(0)
    ts[0].cancel()
    s.release()
    await asyncio.sleep(0)
    log([t.done() and (t.cancelled() or t.result()) for t in ts], s.locked())
    s.release()
    try:
        s.release(); s.release()
    except ValueError as e:
        log(e)
    for t in ts: t.cancel()

async def leftovers():
    asyncio.create_task(asyncio.sleep(10), name='sleeper')
    async def stubborn():
        try:
            await asyncio.sleep(10)
        except asyncio.CancelledError:
            log('stubborn cancelled')
    asyncio.create_task(stubborn(), name='stub')
    loop = asyncio.get_running_loop()
    loop.call_later(10, log, 'never')
    loop.call_soon(log, 'soon at exit')
    await asyncio.sleep(0)

async def main():
    await lock_cancel()
    await event_cond()
    await queue_cancel()
    await semas()
    await leftovers()
    log('main end')

asyncio.run(main())
log('after run')


async def reprs():
    loop = asyncio.get_running_loop()
    for v in (None, list(range(10)), 'x' * 50, {i: str(i) for i in range(6)}, 10 ** 50,
              [[[[[[[1]]]]]]], (1,), set(), frozenset({3, 1, 2})):
        f = loop.create_future()
        f.set_result(v)
        log(f)
    f = loop.create_future()
    log(f, [f])
    f.set_exception(KeyError('k'))
    log(f)
    f.exception()
    f = loop.create_future()
    f.cancel()
    log(f)
    t = asyncio.create_task(asyncio.sleep(0), name=42)
    t2 = asyncio.create_task(asyncio.sleep(0))
    log(t.get_name(), t2.get_name())
    await t
    await t2


asyncio.run(reprs())
