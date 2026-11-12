import asyncio
import json

hosts = json.loads(await list_hosts({}))
responses = await asyncio.gather(*(get_metrics({"host": item["host"], "metric": "cpu"}) for item in hosts))
means = []
for item, response in zip(hosts, responses):
    values = [point[1] for point in json.loads(response)["points"] if point[1] is not None]
    if values:
        means.append({"host": item["host"], "mean_cpu": round(sum(values) / len(values), 1)})
top = sorted(means, key=lambda item: (-item["mean_cpu"], item["host"]))[:3]
await submit_answer({"hosts": top})
