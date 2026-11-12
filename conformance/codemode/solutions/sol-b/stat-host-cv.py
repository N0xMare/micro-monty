import asyncio
import json
import statistics
hosts = json.loads(await list_hosts({}))
series = await asyncio.gather(*(get_metrics({"host": h["host"], "metric": "cpu"}) for h in hosts))
variation = {}
for host, raw in zip(hosts, series):
    values = [value for t, value in json.loads(raw)["points"] if value is not None]
    variation[host["host"]] = statistics.stdev(values) / statistics.mean(values)
await submit_answer({"most_variable": max(variation, key=variation.get), "least_variable": min(variation, key=variation.get)})
