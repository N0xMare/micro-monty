hosts = [h["host"] for h in await list_hosts({})]
series = await asyncio.gather(*(get_metrics({"host": h, "metric": "cpu"}) for h in hosts))
means = []
for h, s in zip(hosts, series):
    vals = [v for _, v in s["points"] if v is not None]
    if vals:
        means.append((h, sum(vals) / len(vals)))
means.sort(key=lambda x: (-x[1], x[0]))
top = [{"host": h, "mean_cpu": round(m, 1)} for h, m in means[:3]]
print(top)
await submit_answer({"hosts": top})
