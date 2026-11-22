# Callbacks queued as asyncio.run's main coroutine ends, with no task left.
import asyncio


async def sleeper():
    await asyncio.sleep(1000)


async def selfc(a, tag):
    asyncio.current_task().cancel()
    try:
        await a
    except asyncio.CancelledError:
        print(tag, 'caught')
        return 'caught'


async def main(n, tag):
    s = asyncio.create_task(sleeper())
    c = asyncio.create_task(selfc(s, tag))
    c.add_done_callback(lambda f: print(tag, 'cb', f.cancelled()))
    s.add_done_callback(lambda f: print(tag, 'scb', f.cancelled()))
    for i in range(n):
        await asyncio.sleep(0)
        print(tag, i, s.done(), c.done())


for n in range(0, 5):
    asyncio.run(main(n, n))
    print(n, 'end')


# How far a chain of call_soon callbacks gets after the main coroutine ended.
async def chain(k, tag):
    loop = asyncio.get_running_loop()

    def step(i):
        print(tag, 'step', i)
        loop.call_soon(step, i + 1)

    async def starter():
        for _ in range(k):
            await asyncio.sleep(0)
        loop.call_soon(step, 0)

    asyncio.create_task(starter())
    await asyncio.sleep(0)


for k in range(0, 4):
    asyncio.run(chain(k, 'chain%d' % k))
    print(k, 'end')


# The same with a task left for the run to cancel.
async def chain_left(tag):
    loop = asyncio.get_running_loop()

    def step(i):
        print(tag, 'step', i)
        loop.call_soon(step, i + 1)

    asyncio.create_task(sleeper())
    loop.call_soon(step, 0)


asyncio.run(chain_left('left'))
print('left end')
