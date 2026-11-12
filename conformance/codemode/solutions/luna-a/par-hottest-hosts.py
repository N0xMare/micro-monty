import asyncio
import json

hosts = json.loads(await list_hosts({}))
results = await asyncio.gather(*(get_metrics({"host": item["host"], "metric": "cpu"}) for item in hosts))
means = []
for item, result in zip(hosts, results):
    points = json.loads(result)["points"]
    values = [value for _, value in points if value is not None]
    means.append((item["host"], sum(values) / len(values)))
top = [{"host": host, "mean_cpu": round(mean, 1)} for host, mean in sorted(means, key=lambda row: (-row[1], row[0]))[:3]]
await submit_answer({"hosts": top})
