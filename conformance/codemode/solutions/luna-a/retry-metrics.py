import asyncio
import json

async def get_with_retries(host):
    for attempt in range(4):
        try:
            return await get_metrics({"host": host, "metric": "mem"})
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise

hosts = json.loads(await list_hosts({}))
db_hosts = sorted(host["host"] for host in hosts if host["host"].startswith("db-"))
results = await asyncio.gather(*(get_with_retries(host) for host in db_hosts))
means = {}
for host, result in zip(db_hosts, results):
    values = [value for _, value in json.loads(result)["points"] if value is not None]
    means[host] = round(sum(values) / len(values), 1) if values else None
await submit_answer({"means": means})
