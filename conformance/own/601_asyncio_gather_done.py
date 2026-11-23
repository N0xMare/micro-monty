# When a gather is done: in the done callback of its last child to
# notify, or without return_exceptions of its first failed or cancelled
# one, as CPython's _done_callback counts, not when that child settles.
# Children that settle in another order than their notifications are
# queued, children already settled (or cancelled) when the gather is
# made, a child named twice, nested gathers, a gather cancelled, and the
# order of the gather's own done callbacks and waiters.
import asyncio

log = []


async def child(name, n, fail=False):
    for _ in range(n):
        await asyncio.sleep(0)
    if fail:
        raise ValueError(name)
    return name


async def wait_on(a):
    return await a


async def watch(name, g):
    try:
        log.append((name, 'woke', await g))
    except BaseException as e:
        log.append((name, 'woke', type(e).__name__, e.args))


def show(title):
    print(title)
    for x in log:
        print('  ', x)
    log.clear()


async def steps(n, *named):
    for i in range(n):
        await asyncio.sleep(0)
        log.append((i,) + tuple(a.done() for _, a in named))


async def main():
    loop = asyncio.get_running_loop()

    # The later-made task settles first: its notification is queued
    # after the other's.
    tb = asyncio.create_task(child('b', 2))
    ta = asyncio.create_task(child('a', 1))
    g = asyncio.gather(ta, tb)
    g.add_done_callback(lambda f: log.append('callback of g'))
    w = asyncio.create_task(watch('w', g))
    await steps(6, ('a', ta), ('b', tb), ('g', g), ('w', w))
    show('out of order:')

    # A failure the same way, with and without return_exceptions.
    for keep in (False, True):
        tb = asyncio.create_task(child('b', 2, fail=True))
        ta = asyncio.create_task(child('a', 1))
        g = asyncio.gather(ta, tb, return_exceptions=keep)
        w = asyncio.create_task(watch('w', g))
        await steps(6, ('a', ta), ('b', tb), ('g', g), ('w', w))
        show(f'a failure, return_exceptions={keep}:')

    # Two failures: the gather takes the error of the first to notify.
    tb = asyncio.create_task(child('b', 2, fail=True))
    ta = asyncio.create_task(child('a', 1, fail=True))
    g = asyncio.gather(tb, ta)
    w = asyncio.create_task(watch('w', g))
    await steps(6, ('g', g), ('w', w))
    show('two failures:')

    # A future and a task that waits on it.
    f = loop.create_future()
    t = asyncio.create_task(wait_on(f))
    await asyncio.sleep(0)
    g = asyncio.gather(f, t)
    g.add_done_callback(lambda fut: log.append('callback of g'))
    f.set_result(1)
    await steps(3, ('t', t), ('g', g))
    show('a future and its waiter:')

    # Children settled before the gather is made, one cancelled.
    done = loop.create_future()
    done.set_result(3)
    gone = loop.create_future()
    gone.cancel('m')
    for keep in (False, True):
        g = asyncio.gather(done, gone, return_exceptions=keep)
        w = asyncio.create_task(watch('w', g))
        await steps(3, ('g', g), ('w', w))
        show(f'settled before, return_exceptions={keep}:')
    bad = loop.create_future()
    bad.set_exception(KeyError('k'))
    p = loop.create_future()
    g = asyncio.gather(p, bad)
    w = asyncio.create_task(watch('w', g))
    await steps(2, ('g', g), ('w', w))
    p.set_result(0)
    await steps(2, ('g', g), ('w', w))
    show('failed before:')
    p = loop.create_future()
    g = asyncio.gather(done, p, done)
    w = asyncio.create_task(watch('w', g))
    await steps(2, ('g', g), ('w', w))
    p.set_result(1)
    await steps(3, ('g', g), ('w', w))
    show('settled before, named twice:')

    # A child named twice, settled out of order with a third.
    tb = asyncio.create_task(child('b', 2))
    ta = asyncio.create_task(child('a', 1))
    g = asyncio.gather(tb, ta, tb, ta)
    w = asyncio.create_task(watch('w', g))
    await steps(6, ('g', g), ('w', w))
    show('named twice:')

    # Nested gathers: the inner one's notification to the outer is
    # queued only when the inner is done.
    tb = asyncio.create_task(child('b', 2))
    ta = asyncio.create_task(child('a', 1))
    inner = asyncio.gather(ta, tb)
    tc = asyncio.create_task(child('c', 1))
    outer = asyncio.gather(inner, tc)
    w = asyncio.create_task(watch('w', outer))
    await steps(6, ('inner', inner), ('outer', outer), ('w', w))
    show('nested:')

    # A gather cancelled while one child waits on the other.
    async def sleeper():
        await asyncio.sleep(1000)

    s = asyncio.create_task(sleeper())
    t = asyncio.create_task(wait_on(s))
    await asyncio.sleep(0)
    for keep in (True, False):
        g = asyncio.gather(s, t, return_exceptions=keep)
        log.append(('cancel', g.cancel('m')))
        w = asyncio.create_task(watch('w', g))
        await steps(4, ('s', s), ('t', t), ('g', g), ('w', w))
        show(f'cancelled, return_exceptions={keep}:')
        s = asyncio.create_task(sleeper())
        t = asyncio.create_task(wait_on(s))
        await asyncio.sleep(0)
    s.cancel()
    await asyncio.gather(s, t, return_exceptions=True)


asyncio.run(main())
