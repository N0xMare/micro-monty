import asyncio, json

async def retry_metrics(host):
    for attempt in range(4):
        try:
            return json.loads(await get_metrics({"host": host, "metric": "mem"}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise
            await asyncio.sleep(0)

hosts = json.loads(await list_hosts({}))
db_hosts = sorted(host["host"] for host in hosts if host["host"].startswith("db-"))
series = await asyncio.gather(*(retry_metrics(host) for host in db_hosts))
means = {}
for host, result in zip(db_hosts, series):
    values = [value for _, value in result["points"] if value is not None]
    means[host] = round(sum(values) / len(values), 1) if values else None
await submit_answer({"means": means})
