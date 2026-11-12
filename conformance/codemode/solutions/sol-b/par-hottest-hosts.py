import asyncio
import json
from statistics import mean

hosts = json.loads(await list_hosts({}))
raw = await asyncio.gather(*(get_metrics({"host": host["host"], "metric": "cpu"}) for host in hosts))
ranked = []
for host, result in zip(hosts, raw):
    values = [value for ts, value in json.loads(result)["points"] if value is not None]
    if values:
        ranked.append((host["host"], mean(values)))
ranked.sort(key=lambda item: (-item[1], item[0]))
await submit_answer({"hosts": [{"host": host, "mean_cpu": round(average, 1)} for host, average in ranked[:3]]})
