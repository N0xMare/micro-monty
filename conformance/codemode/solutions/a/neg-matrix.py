import asyncio, json, math

metrics = ["cpu", "mem", "latency_ms"]
raw = await asyncio.gather(*(get_metrics({"host": "web-02", "metric": m}) for m in metrics))
series = [{ts: v for ts, v in json.loads(r)["points"] if v is not None} for r in raw]
common = sorted(set.intersection(*(set(s) for s in series)))
cols = [[s[t] for t in common] for s in series]
print("points with all three present:", len(common))

def pearson(x, y):
    n = len(x)
    mx, my = sum(x) / n, sum(y) / n
    sxy = sum((a - mx) * (b - my) for a, b in zip(x, y))
    sxx = sum((a - mx) ** 2 for a in x)
    syy = sum((b - my) ** 2 for b in y)
    return sxy / math.sqrt(sxx * syy)

matrix = [[1.0 if i == j else round(pearson(cols[i], cols[j]), 3) for j in range(3)] for i in range(3)]
for name, row in zip(metrics, matrix):
    print(name, row)
print(await submit_answer({"matrix": matrix}))
