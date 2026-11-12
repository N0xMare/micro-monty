import asyncio
import json


def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))


async def call(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)


hosts = json.loads(await call(list_hosts, {}))
db_hosts = [h["host"] for h in hosts if h["host"].startswith("db-")]
raws = await asyncio.gather(*(call(get_metrics, {"host": h, "metric": "mem"}) for h in db_hosts))

means = {}
for h, raw in zip(db_hosts, raws):
    values = [v for _, v in json.loads(raw)["points"] if v is not None]
    means[h] = round(sum(values) / len(values), 1) if values else None

print(means)
print(await submit_answer({"means": means}))
