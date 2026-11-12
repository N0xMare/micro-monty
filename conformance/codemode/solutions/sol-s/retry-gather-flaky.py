import asyncio

async def fetch(host):
    for attempt in range(4):
        try:
            return await get_metrics({"host": host, "metric": "cpu"})
        except (TimeoutError, RuntimeError) as error:
            if not (isinstance(error, TimeoutError) or "503" in str(error)) or attempt == 3:
                raise

hosts = ["web-01", "web-02", "web-03", "web-04"]
results = await asyncio.gather(*(fetch(h) for h in hosts))
await submit_answer({"max_cpu": {h: max(v for t, v in r["points"] if v is not None) for h, r in zip(hosts, results)}})
