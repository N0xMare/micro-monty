import asyncio
async def worker(q, lock, out):
    while True:
        i = await q.get()
        async with lock:
            out.append(i)
        q.task_done()
async def main():
    q, lock, out = asyncio.Queue(), asyncio.Lock(), []
    sem = asyncio.Semaphore(2)
    async with asyncio.TaskGroup() as tg:
        for i in range(5):
            q.put_nowait(i)
        ws = [asyncio.create_task(worker(q, lock, out)) for _ in range(2)]
        await asyncio.wait_for(q.join(), 1)
        for w in ws:
            w.cancel()
    async with sem:
        return sorted(out)
print(asyncio.run(main()))
