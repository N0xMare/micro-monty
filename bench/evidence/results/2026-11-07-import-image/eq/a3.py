q = None
import asyncio
async def main():
    q = asyncio.Queue(); await q.put(1)
    pq = asyncio.PriorityQueue(); await pq.put((2, "b")); await pq.put((1, "a"))
    async with asyncio.TaskGroup() as tg:
        t = tg.create_task(asyncio.sleep(0, "x"))
    return await q.get(), await pq.get(), t.result(), issubclass(asyncio.PriorityQueue, asyncio.Queue)
asyncio.run(main())
