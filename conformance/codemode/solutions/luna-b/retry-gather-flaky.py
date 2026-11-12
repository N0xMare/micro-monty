import asyncio, json

async def fetch(host):
    for attempt in range(4):
        try:
            return json.loads(await get_metrics({"host": host, "metric": "cpu"}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise
            await asyncio.sleep(0)

hosts = ["web-01", "web-02", "web-03", "web-04"]
series = await asyncio.gather(*(fetch(host) for host in hosts))
max_cpu = {host: max((value for _, value in result["points"] if value is not None), default=None)
           for host, result in zip(hosts, series)}
await submit_answer({"max_cpu": max_cpu})
