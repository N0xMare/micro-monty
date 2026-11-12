import asyncio, json

hosts = [h["host"] for h in json.loads(await list_hosts({}))]
results = await asyncio.gather(*(get_metrics({"host": h, "metric": "cpu"}) for h in hosts))
means = {}
for h, r in zip(hosts, results):
    vals = [v for _, v in json.loads(r)["points"] if v is not None]
    if vals:
        means[h] = sum(vals) / len(vals)

top = sorted(means.items(), key=lambda kv: -kv[1])[:3]
answer = [{"host": h, "mean_cpu": round(m, 1)} for h, m in top]
print(answer)
print(await submit_answer({"hosts": answer}))
