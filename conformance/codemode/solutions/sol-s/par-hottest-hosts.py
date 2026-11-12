import asyncio

hosts = await list_hosts({})
results = await asyncio.gather(*(get_metrics({"host": host["host"], "metric": "cpu"}) for host in hosts))
means = []
for host, result in zip(hosts, results):
    values = [value for ts, value in result["points"] if value is not None]
    if values:
        means.append((host["host"], sum(values) / len(values)))
means.sort(key=lambda pair: (-pair[1], pair[0]))
await submit_answer({"hosts": [{"host": host, "mean_cpu": round(mean, 1)} for host, mean in means[:3]]})
