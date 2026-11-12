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

hosts = ["web-01", "web-02", "web-03", "web-04"]
results = await asyncio.gather(*[with_retry(get_metrics, {"host": h, "metric": "cpu"}) for h in hosts])

max_cpu = {}
for h, raw in zip(hosts, results):
    vals = [v for _, v in json.loads(raw)["points"] if v is not None]
    max_cpu[h] = max(vals) if vals else None

print(max_cpu)
print(await submit_answer({"max_cpu": max_cpu}))
