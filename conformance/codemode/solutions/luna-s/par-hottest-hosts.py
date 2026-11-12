import asyncio

hosts = await list_hosts({})
series = await asyncio.gather(*(get_metrics({"host": h["host"], "metric": "cpu"}) for h in hosts))
means = []
for host, result in zip(hosts, series):
    samples = [value for _, value in result["points"] if value is not None]
    if samples:
        means.append({"host": host["host"], "mean_cpu": round(sum(samples) / len(samples), 1)})
top = sorted(means, key=lambda row: (-row["mean_cpu"], row["host"]))[:3]
await submit_answer({"hosts": top})
