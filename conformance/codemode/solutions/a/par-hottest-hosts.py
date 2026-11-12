import json
import asyncio

hosts = [h["host"] for h in json.loads(await list_hosts({}))]
results = await asyncio.gather(*(get_metrics({"host": h, "metric": "cpu"}) for h in hosts))
means = []
for h, r in zip(hosts, results):
    vals = [v for _, v in json.loads(r)["points"] if v is not None]
    if vals:
        means.append((h, sum(vals) / len(vals)))
means.sort(key=lambda x: -x[1])
top = [{"host": h, "mean_cpu": round(m, 1)} for h, m in means[:3]]
print(top)
await submit_answer({"hosts": top})
