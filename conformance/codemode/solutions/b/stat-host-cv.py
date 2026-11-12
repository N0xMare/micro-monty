import asyncio
import json
import statistics

hosts = [h["host"] for h in json.loads(await list_hosts({}))]
series = await asyncio.gather(*(get_metrics({"host": h, "metric": "cpu"}) for h in hosts))

cv = {}
for h, raw in zip(hosts, series):
    values = [v for _, v in json.loads(raw)["points"] if v is not None]
    if len(values) < 2 or statistics.mean(values) == 0:
        print(h, "skipped:", len(values), "samples")
        continue
    cv[h] = statistics.stdev(values) / statistics.mean(values)

for h, c in sorted(cv.items(), key=lambda kv: kv[1]):
    print(f"{h}: {c:.4f}")
answer = {"most_variable": max(cv, key=cv.get), "least_variable": min(cv, key=cv.get)}
print(answer)
print(await submit_answer(answer))
