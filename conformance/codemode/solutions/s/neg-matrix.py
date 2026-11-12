import asyncio
import math

metrics = ["cpu", "mem", "latency_ms"]
series = await asyncio.gather(*(get_metrics({"host": "web-02", "metric": m}) for m in metrics))
maps = [{t: v for t, v in s["points"] if v is not None} for s in series]
common = sorted(set(maps[0]) & set(maps[1]) & set(maps[2]))
cols = [[mp[t] for t in common] for mp in maps]
print(f"{len(common)} timestamps with all three present")

def pearson(x, y):
    n = len(x)
    mx, my = sum(x) / n, sum(y) / n
    sxy = sum((a - mx) * (b - my) for a, b in zip(x, y))
    sxx = sum((a - mx) ** 2 for a in x)
    syy = sum((b - my) ** 2 for b in y)
    return sxy / math.sqrt(sxx * syy)

matrix = [[1.0 if i == j else round(pearson(cols[i], cols[j]), 3) for j in range(3)]
          for i in range(3)]
for name, row in zip(metrics, matrix):
    print(name.ljust(11), row)
print(await submit_answer({"matrix": matrix}))
