# Task names, many failed tasks, and cancelling wait_for and as_completed.
import asyncio
async def boom(i):
    raise ValueError(i)
async def main():
    t = asyncio.create_task(asyncio.sleep(0), name='first')
    t.set_name('renamed')
    print(t.get_name())
    await t
    ts = [asyncio.ensure_future(boom(i)) for i in range(70)]
    await asyncio.sleep(0)
    await asyncio.sleep(0)
    print(sum(t.done() for t in ts))
    for t in ts:
        t.exception()
    async def waiter():
        await asyncio.wait_for(asyncio.sleep(10), 5)
    w = asyncio.create_task(waiter())
    await asyncio.sleep(0)
    w.cancel()
    try:
        await w
    except asyncio.CancelledError:
        print('cancelled')
    async def ac():
        for f in asyncio.as_completed([asyncio.sleep(10)]):
            await f
    w = asyncio.create_task(ac())
    await asyncio.sleep(0)
    w.cancel()
    try:
        await w
    except asyncio.CancelledError:
        print('cancelled2')
asyncio.run(main())
