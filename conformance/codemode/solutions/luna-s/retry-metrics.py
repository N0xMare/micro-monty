import asyncio
async def retry_call(function, arguments):
    for attempt in range(4):
        try:
            return await function(arguments)
        except (TimeoutError, RuntimeError) as error:
            if isinstance(error, RuntimeError) and "503" not in str(error):
                raise
            if attempt == 3:
                raise
hosts = await retry_call(list_hosts, {})
db_hosts = [item["host"] for item in hosts if item["host"].startswith("db-")]
series = await asyncio.gather(*(retry_call(get_metrics, {"host": host, "metric": "mem"}) for host in db_hosts))
means = {}
for item in series:
    values = [value for _, value in item["points"] if value is not None]
    means[item["host"]] = round(sum(values) / len(values), 1) if values else None
await submit_answer({"means": means})
