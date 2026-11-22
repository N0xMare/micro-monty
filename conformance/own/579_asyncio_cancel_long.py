# A cancel whose tree is longer than one step of its walk: what takes the
# cancel, the order the callbacks run in and what each await raises. A
# gather of many futures with done callbacks; tasks waiting on futures
# inside nested gathers, and a task whose waiter is such a gather; a tree
# already done; awaitables reached twice and a second cancel over the
# same tree; tasks whose waiter settled first; many sleeping tasks; and
# the two cancels the loop itself makes: for a task that blocks with a
# cancel pending, and for the main task when SystemExit leaves a task.
import asyncio

log = []


def note(f):
    log.append(f.cancelled())


async def wait_on(f):
    try:
        await f
    except asyncio.CancelledError:
        return 'task saw the cancel'


async def flat():
    loop = asyncio.get_running_loop()
    fs = [loop.create_future() for _ in range(40_000)]
    for f in fs:
        f.add_done_callback(note)
    fs[7].set_result('settled first')
    g = asyncio.gather(*fs)
    print(g.cancel('flat'), g.cancel())
    try:
        await g
    except asyncio.CancelledError as e:
        print('flat', e.args)
    print(len(log), sum(log), fs[7].result(), fs[8].cancelled())


async def nested():
    loop = asyncio.get_running_loop()
    inner = [loop.create_future() for _ in range(25_000)]
    tasks = [asyncio.create_task(wait_on(f)) for f in inner[:10_000]]
    await asyncio.sleep(0)
    g = asyncio.gather(asyncio.gather(*tasks), asyncio.gather(*inner[10_000:]))
    outer = asyncio.create_task(wait_on(g))
    await asyncio.sleep(0)
    print(outer.cancel('nested'))
    print(await outer)
    print(sum(f.cancelled() for f in inner), sum(t.cancelled() for t in tasks))
    print(g.cancelled(), [t.result() for t in tasks[:2]], tasks[0].cancelling())


async def done_tree():
    loop = asyncio.get_running_loop()
    fs = [loop.create_future() for _ in range(20_000)]
    for f in fs:
        f.set_result(1)
    g = asyncio.gather(*fs)
    await asyncio.sleep(0)
    print(g.cancel(), sum(await g))


async def shared():
    # Futures in two gathers and twice in one, two tasks on one future,
    # and a second cancel while the first one's callbacks are queued.
    loop = asyncio.get_running_loop()
    fs = [loop.create_future() for _ in range(30_000)]
    order = []
    fs[0].add_done_callback(lambda f: order.append('first future'))
    fs[-1].add_done_callback(lambda f: order.append('last future'))
    a = asyncio.gather(*fs[:20_000], *fs[:5])
    b = asyncio.gather(*fs[10_000:])
    t1 = asyncio.create_task(wait_on(fs[25_000]))
    t2 = asyncio.create_task(wait_on(fs[25_000]))
    both = asyncio.gather(a, b, t1, t2, return_exceptions=True)
    a.add_done_callback(lambda f: order.append('a'))
    b.add_done_callback(lambda f: order.append('b'))
    await asyncio.sleep(0)
    print(a.cancel('one'), sum(f.cancelled() for f in fs), b.done(), order)
    print(both.cancel('two'), sum(f.cancelled() for f in fs), a.cancel(), order)
    print(t1.cancelling(), t2.cancelling(), t1.done())
    r = await asyncio.gather(both, return_exceptions=True)
    print([type(x).__name__ for x in r], order)
    print(t1.result(), t2.result(), a.cancelled(), b.cancelled())
    try:
        await a
    except asyncio.CancelledError as e:
        print('a', e.args)


async def settled_waiters():
    # Tasks whose future settled before the cancel, not yet woken: the
    # future cannot take it, so each such task raises at its next step.
    loop = asyncio.get_running_loop()
    fs = [loop.create_future() for _ in range(20_000)]
    ts = [asyncio.create_task(wait_on(f)) for f in fs]
    await asyncio.sleep(0)
    for f in fs[::2]:
        f.set_result('set')
    g = asyncio.gather(*ts)
    print(g.cancel('late'), sum(f.cancelled() for f in fs))
    r = await asyncio.gather(g, return_exceptions=True)
    print(type(r[0]).__name__, [t.result() for t in ts[:4]], [t.cancelling() for t in ts[:2]])
    print(sum(t.result() == 'task saw the cancel' for t in ts), sum(t.cancelled() for t in ts))


async def sleeper(out, i):
    try:
        await asyncio.sleep(1000)
    except asyncio.CancelledError:
        out.append(i)
        raise


async def sleepers():
    out = []
    ts = [asyncio.create_task(sleeper(out, i)) for i in range(3_000)]
    await asyncio.sleep(0)
    g = asyncio.gather(*ts)
    print(g.cancel('wake'), len(out))
    try:
        await g
    except asyncio.CancelledError as e:
        print('sleepers', e.args, len(out), out[:3], out[-1], out == sorted(out))
    print(sum(t.cancelled() for t in ts))


async def blocks_cancelled():
    # A task cancelled while it runs passes the cancel to what it then
    # waits on: the loop cancels that tree when the task blocks.
    loop = asyncio.get_running_loop()
    fs = [loop.create_future() for _ in range(40_000)]
    order = []
    fs[0].add_done_callback(lambda f: order.append('first future'))
    fs[-1].add_done_callback(lambda f: order.append('last future'))
    g = asyncio.gather(*fs)

    async def body():
        me.cancel('while running')
        order.append('body goes on')
        try:
            await g
        except asyncio.CancelledError as e:
            order.append(('body', e.args, me.cancelling()))
            return 'body caught it'

    async def other():
        order.append('other task')

    me = asyncio.create_task(body())
    asyncio.create_task(other())
    print(await me, order)
    print(g.cancelled(), sum(f.cancelled() for f in fs))

    # What it then waits on is done already: it raises at its next step.
    done = loop.create_future()
    done.set_result(3)

    async def late():
        cur.cancel('late')
        try:
            return await asyncio.gather(done, *[asyncio.sleep(0) for _ in range(3)])
        except asyncio.CancelledError as e:
            return ('late', e.args)

    cur = asyncio.create_task(late())
    print(await cur)

    # What it then waits on takes the cancel and ends normally: the task
    # is not cancelled again.
    inner = [loop.create_future() for _ in range(40_000)]
    kid = asyncio.create_task(wait_on(asyncio.gather(*inner)))
    await asyncio.sleep(0)

    async def calm():
        calmed.cancel('calm')
        return await kid, calmed.cancelling()

    calmed = asyncio.create_task(calm())
    print(await calmed, calmed.cancelled(), sum(f.cancelled() for f in inner))


async def task_twice():
    # A task reached many times takes each request (`cancelling()`), and
    # what it waits on the first.
    loop = asyncio.get_running_loop()
    f = loop.create_future()
    f.add_done_callback(note)
    t = asyncio.create_task(wait_on(f))
    await asyncio.sleep(0)
    g = asyncio.gather(*[t] * 20_000)
    print('a task reached many times:', g.cancel('many'), t.cancelling(), f.cancelled())
    try:
        await g
    except asyncio.CancelledError:
        print('its gather cancelled:', await t, t.cancelled(), log[-1])


left = []


async def escapes():
    loop = asyncio.get_running_loop()
    fs = [loop.create_future() for _ in range(40_000)]
    fs[-1].add_done_callback(lambda f: print('last future cancelled', f.cancelled()))

    async def leave():
        raise SystemExit('left the loop')

    left.append(asyncio.create_task(leave()))
    try:
        await asyncio.gather(*fs)
    except asyncio.CancelledError:
        print('main cancelled', sum(f.cancelled() for f in fs))
        raise


asyncio.run(flat())
asyncio.run(nested())
asyncio.run(done_tree())
asyncio.run(shared())
asyncio.run(settled_waiters())
asyncio.run(sleepers())
asyncio.run(blocks_cancelled())
asyncio.run(task_twice())
try:
    asyncio.run(escapes())
except SystemExit as e:
    print('SystemExit', e.args, left[0].exception() is e)
