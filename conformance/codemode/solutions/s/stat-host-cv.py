import asyncio
import statistics

hosts = [h["host"] for h in await list_hosts({})]
series = await asyncio.gather(*[get_metrics({"host": h, "metric": "cpu"}) for h in hosts])

cv = {}
for h, s in zip(hosts, series):
    vals = [v for _, v in s["points"] if v is not None]
    if len(vals) < 2:
        print(h, "too few samples")
        continue
    mean = statistics.mean(vals)
    if mean == 0:
        print(h, "zero mean")
        continue
    cv[h] = statistics.stdev(vals) / mean
    print(h, len(vals), round(mean, 3), round(cv[h], 4))

most = max(cv, key=cv.get)
least = min(cv, key=cv.get)
await submit_answer({"most_variable": most, "least_variable": least})
