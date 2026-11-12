import asyncio
import json
from statistics import mean

hosts = json.loads(await list_hosts({}))
results = await asyncio.gather(*(get_metrics({"host": host["host"], "metric": "cpu"}) for host in hosts))
averages = []
for host, result in zip(hosts, results):
    values = [value for ts, value in json.loads(result)["points"] if value is not None]
    if values:
        averages.append((host["host"], mean(values)))
averages.sort(key=lambda item: (-item[1], item[0]))
await submit_answer({"hosts": [{"host": host, "mean_cpu": round(value, 1)} for host, value in averages[:3]]})
