import asyncio
import json

async def get_with_retries(host):
    for attempt in range(4):
        try:
            return await get_metrics({"host": host, "metric": "cpu"})
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise

hosts = ["web-01", "web-02", "web-03", "web-04"]
results = await asyncio.gather(*(get_with_retries(host) for host in hosts))
max_cpu = {}
for host, result in zip(hosts, results):
    values = [value for _, value in json.loads(result)["points"] if value is not None]
    max_cpu[host] = max(values) if values else None
await submit_answer({"max_cpu": max_cpu})
