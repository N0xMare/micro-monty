import asyncio, json

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def with_retry(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)

hosts = json.loads(await with_retry(list_hosts, {}))
db_hosts = sorted(h["host"] for h in hosts if h["host"].startswith("db-"))
results = await asyncio.gather(*[with_retry(get_metrics, {"host": h, "metric": "mem"}) for h in db_hosts])

means = {}
for h, raw in zip(db_hosts, results):
    vals = [v for _, v in json.loads(raw)["points"] if v is not None]
    means[h] = round(sum(vals) / len(vals), 1) if vals else None
    print(h, len(vals), "samples, mean", means[h])

print(await submit_answer({"means": means}))
