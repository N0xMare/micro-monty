import asyncio
import json

async def retry(tool, args):
    for attempt in range(4):
        try:
            return await tool(args)
        except (TimeoutError, RuntimeError) as error:
            if attempt == 3 or not (isinstance(error, TimeoutError) or "503" in str(error)):
                raise

hosts = [h["host"] for h in json.loads(await retry(list_hosts, {})) if h["host"].startswith("db-")]
series = [json.loads(r) for r in await asyncio.gather(*(retry(get_metrics, {"host": host, "metric": "mem"}) for host in hosts))]
means = {}
for host, data in zip(hosts, series):
    values = [v for _, v in data["points"] if v is not None]
    means[host] = round(sum(values) / len(values), 1)
await submit_answer({"means": means})
