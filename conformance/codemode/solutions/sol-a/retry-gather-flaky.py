import asyncio
import json

async def fetch(host):
    for attempt in range(4):
        try:
            raw = await get_metrics({"host": host, "metric": "cpu"})
        except (TimeoutError, RuntimeError) as error:
            if attempt == 3 or not (isinstance(error, TimeoutError) or "503" in str(error)):
                raise
        else:
            return json.loads(raw)

hosts = ["web-01", "web-02", "web-03", "web-04"]
series = await asyncio.gather(*(fetch(h) for h in hosts))
await submit_answer({"max_cpu": {host: max(v for _, v in data["points"] if v is not None) for host, data in zip(hosts, series)}})
