import asyncio, json

async def retry(tool, args):
    for attempt in range(4):
        try:
            return await tool(args)
        except (TimeoutError, RuntimeError) as exc:
            if not (isinstance(exc, TimeoutError) or "503" in str(exc)) or attempt == 3:
                raise

hosts = [h["host"] for h in json.loads(await retry(list_hosts, {})) if h["host"].startswith("db-")]
series = [json.loads(s) for s in await asyncio.gather(*(retry(get_metrics, {"host": h, "metric": "mem"}) for h in hosts))]
means = {}
for host, data in zip(hosts, series):
    values = [v for t, v in data["points"] if v is not None]
    means[host] = round(sum(values) / len(values), 1)
await submit_answer({"means": means})
