import asyncio
import json
import statistics
hosts = json.loads(await list_hosts({}))
async def variability(host):
    points = json.loads(await get_metrics({"host": host["host"], "metric": "cpu"}))["points"]
    values = [v for t, v in points if v is not None]
    return host["host"], statistics.stdev(values) / statistics.mean(values)
values = await asyncio.gather(*(variability(host) for host in hosts))
await submit_answer({"most_variable": max(values, key=lambda x: x[1])[0], "least_variable": min(values, key=lambda x: x[1])[0]})
