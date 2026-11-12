import asyncio
import math

metrics = ["cpu", "mem", "latency_ms"]
series = await asyncio.gather(*(get_metrics({"host": "web-02", "metric": metric}) for metric in metrics))
maps = [{timestamp: value for timestamp, value in result["points"] if value is not None} for result in series]
common = set(maps[0]).intersection(maps[1], maps[2])
values = [[mapping[t] for t in sorted(common)] for mapping in maps]

def corr(x, y):
    mx, my = sum(x) / len(x), sum(y) / len(y)
    dx, dy = [v - mx for v in x], [v - my for v in y]
    denom = math.sqrt(sum(v * v for v in dx) * sum(v * v for v in dy))
    return sum(a * b for a, b in zip(dx, dy)) / denom

matrix = [[round(corr(x, y), 3) for y in values] for x in values]
await submit_answer({"matrix": matrix})
