import asyncio

async def retry(tool, args):
    for attempt in range(4):
        try:
            return await tool(args)
        except (TimeoutError, RuntimeError) as error:
            if not (isinstance(error, TimeoutError) or "503" in str(error)) or attempt == 3:
                raise

hosts = [h["host"] for h in await retry(list_hosts, {}) if h["host"].startswith("db-")]
results = await asyncio.gather(*(retry(get_metrics, {"host": h, "metric": "mem"}) for h in hosts))
means = {}
for h, result in zip(hosts, results):
    values = [v for t, v in result["points"] if v is not None]
    means[h] = round(sum(values) / len(values), 1)
await submit_answer({"means": means})
