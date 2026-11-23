# `uncancel()` on a task already done only lowers its count (CPython 3.14's
# `Task.uncancel` resets `_must_cancel`, which a done task no longer reads):
# later awaits of it raise the CancelledError it ended with, and a task that
# failed after a cancel keeps its exception.
import asyncio
async def passthru(a):
    await a
async def failer(f):
    try:
        await f
    except asyncio.CancelledError:
        raise ValueError('after cancel')
async def main():
    loop = asyncio.get_running_loop()
    t = asyncio.create_task(passthru(loop.create_future()))
    await asyncio.sleep(0)
    t.cancel('m2')
    await asyncio.sleep(0)
    print('done', t.done(), t.cancelled(), t.cancelling())
    print('uncancel', t.uncancel(), t.uncancel(), t.cancelling())
    for _ in range(2):
        try:
            await t
        except asyncio.CancelledError as e:
            print('await', e.args)
    # A waiter that was already blocked on it when it ended.
    u = asyncio.create_task(passthru(loop.create_future()))
    w = asyncio.create_task(passthru(u))
    await asyncio.sleep(0)
    u.cancel('m3')
    u.cancel('m4')
    await asyncio.sleep(0)
    print('uncancel', u.uncancel(), u.uncancel())
    try:
        await w
    except asyncio.CancelledError as e:
        print('waiter', e.args, w.cancelling())
    try:
        u.result()
    except asyncio.CancelledError as e:
        print('result', e.args)
    # Before the task ends, uncancel drops the pending cancel.
    v = asyncio.create_task(passthru(asyncio.sleep(0)))
    v.cancel('dropped')
    print('pending uncancel', v.uncancel())
    print('ran', await v)
    # Failed after a cancel: uncancel leaves its exception.
    x = asyncio.create_task(failer(loop.create_future()))
    await asyncio.sleep(0)
    x.cancel()
    await asyncio.sleep(0)
    print('failed', x.done(), x.uncancel(), repr(x.exception()))
asyncio.run(main())
