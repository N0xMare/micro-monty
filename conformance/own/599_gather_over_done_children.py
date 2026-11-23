# A gather made over children already done runs their done callbacks before
# it returns, in argument order (CPython 3.14): the first that failed or was
# cancelled settles it at once, unless `return_exceptions`, and a gather of
# children all done is done with its list.
import asyncio
async def sleeper():
    await asyncio.sleep(10)
async def raiser():
    raise asyncio.CancelledError('stored')
async def main():
    loop = asyncio.get_running_loop()
    ok = loop.create_future()
    ok.set_result(1)
    bad = loop.create_future()
    bad.set_exception(ValueError('v'))
    c1 = loop.create_future()
    c2 = loop.create_future()
    c2.cancel('second')
    c1.cancel('first')
    p = loop.create_future()
    cases = [
        ('one cancelled', (c1,), False),
        ('cancelled and pending', (p, c1), False),
        ('argument order, not cancel order', (c2, c1), False),
        ('error before cancel', (bad, c1), False),
        ('all done', (ok, ok), False),
        ('kept errors', (ok, bad, c1), True),
        ('kept errors, pending', (p, c1), True),
    ]
    for name, args, keep in cases:
        g = asyncio.gather(*args, return_exceptions=keep)
        print(name, g.done())
        try:
            print(' ', await asyncio.wait_for(g, 0.01))
        except BaseException as e:
            print(' ', type(e).__name__, e.args)
    # A sibling still pending does not hold the failure back.
    t = asyncio.create_task(sleeper())
    await asyncio.sleep(0)
    try:
        await asyncio.gather(t, c1)
    except asyncio.CancelledError as e:
        print('pending sibling', e.args, t.done())
    t.cancel()
    # A task that ended cancelled by raising keeps its error for the first
    # gather made over it: a second gets a new one.
    r = asyncio.create_task(raiser())
    await asyncio.sleep(0)
    g1 = asyncio.gather(r)
    g2 = asyncio.gather(r)
    for g in (g2, g1):
        try:
            await g
        except asyncio.CancelledError as e:
            print('stored', e.args)
asyncio.run(main())
