# aio: an await loop over a coroutine, asyncio.gather over batches of
# coroutines, and tasks from create_task that yield to the event loop.
import asyncio


async def step(i):
    return i * 3 % 7


async def await_loop(n):
    total = 0
    for i in range(n):
        total += await step(i)
    return total


async def worker(k, n):
    acc = 0
    for i in range(n):
        await asyncio.sleep(0)
        acc += (i ^ k) & 15
    return acc


async def main():
    looped = await await_loop(150_000)
    gathered = 0
    for r in range(300):
        values = await asyncio.gather(*(step(r + j) for j in range(50)))
        gathered += sum(values)
    tasks = [asyncio.create_task(worker(k, 1_500)) for k in range(20)]
    done = 0
    for task in tasks:
        done += await task
    print(looped, gathered, done)


asyncio.run(main())
