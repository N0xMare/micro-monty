import asyncio
async def retry_metrics(host):
    for attempt in range(4):
        try:
            return await get_metrics({"host": host, "metric": "cpu"})
        except (TimeoutError, RuntimeError) as error:
            if isinstance(error, RuntimeError) and "503" not in str(error):
                raise
            if attempt == 3:
                raise
hosts = ["web-01", "web-02", "web-03", "web-04"]
series = await asyncio.gather(*(retry_metrics(host) for host in hosts))
max_cpu = {item["host"]: max(value for _, value in item["points"] if value is not None) for item in series}
await submit_answer({"max_cpu": max_cpu})
