import asyncio

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def call(fn, args):
    for attempt in range(4):  # 1 try + 3 retries
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == 3:
                raise
            await asyncio.sleep(0.5 * 2 ** attempt)

hosts = await call(list_hosts, {})
db_hosts = sorted(h["host"] for h in hosts if h["host"].startswith("db-"))
series = await asyncio.gather(*(call(get_metrics, {"host": h, "metric": "mem"}) for h in db_hosts))

means = {}
for h, s in zip(db_hosts, series):
    vals = [v for _, v in s["points"] if v is not None]
    means[h] = round(sum(vals) / len(vals), 1) if vals else None

print(means)
print(await submit_answer({"means": means}))
