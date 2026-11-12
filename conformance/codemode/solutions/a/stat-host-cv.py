import asyncio
import json
import statistics

hosts = [h["host"] for h in json.loads(await list_hosts({}))]
results = await asyncio.gather(*(get_metrics({"host": h, "metric": "cpu"}) for h in hosts))

cvs = {}
for h, raw in zip(hosts, results):
    values = [v for _, v in json.loads(raw)["points"] if v is not None]
    if len(values) < 2:
        print(h, "has too few samples")
        continue
    mean = statistics.mean(values)
    cvs[h] = statistics.stdev(values) / mean

for h, cv in sorted(cvs.items(), key=lambda kv: kv[1]):
    print(h, round(cv, 4))

answer = {"most_variable": max(cvs, key=cvs.get), "least_variable": min(cvs, key=cvs.get)}
print(answer)
print(await submit_answer(answer))
