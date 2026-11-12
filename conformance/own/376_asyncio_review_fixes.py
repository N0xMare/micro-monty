# asyncio corners from a review: sleeps as tasks, as_completed over many
# awaitables, cancelling many tasks, create_task without a running loop,
# and scheduled tasks are not "never awaited".
import asyncio


async def f(i):
    await asyncio.sleep(i * 0.001)
    return i


async def nothing():
    return 1


async def sleeps():
    # A sleep is a coroutine: a task, as_completed and wait_for take it.
    print(await asyncio.create_task(asyncio.sleep(0.01, 'v')))
    async for t in asyncio.as_completed([asyncio.sleep(0.02, 'a'), asyncio.sleep(0.01, 'b')]):
        print(await t)
    for t in asyncio.as_completed([asyncio.sleep(0.01, 'c')]):
        print(await t)
    try:
        await asyncio.wait_for(asyncio.sleep(0.01, 'w'), 0)
    except TimeoutError:
        print('timeout')
    print(await asyncio.wait_for(asyncio.sleep(0.01, 'x'), 1))


async def many():
    # Each step of as_completed waits on every remaining awaitable.
    n = 0
    order = []
    async for t in asyncio.as_completed([f(i) for i in range(300)]):
        i = await t
        n += i
        if len(order) < 5:
            order.append(i)
    print(n, order)


async def cancel_many():
    ts = [asyncio.create_task(nothing()) for _ in range(2000)]
    print(sum(t.cancel() for t in ts[::2]))
    await asyncio.sleep(0)
    print(sum(t.cancelled() for t in ts), sum(t.done() for t in ts))
    print(sum(t.result() for t in ts[1::2]))
    print(ts[0].cancel(), ts[1].cancel())


async def scheduled():
    # Wrapped in tasks: never "never awaited", even if they never run.
    asyncio.create_task(nothing())
    asyncio.gather(nothing(), nothing())
    return 'scheduled'


asyncio.run(sleeps())
asyncio.run(many())
asyncio.run(cancel_many())
print(asyncio.run(scheduled()))

print(asyncio.run(nothing()))
