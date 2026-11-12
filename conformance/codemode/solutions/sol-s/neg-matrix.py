import asyncio
import math

metrics = ("cpu", "mem", "latency_ms")
series = await asyncio.gather(*(get_metrics({"host": "web-02", "metric": metric}) for metric in metrics))
samples = [{ts: value for ts, value in result["points"] if value is not None} for result in series]
times = sorted(set(samples[0]) & set(samples[1]) & set(samples[2]))
columns = [[sample[ts] for ts in times] for sample in samples]
centered = [[value - sum(column) / len(column) for value in column] for column in columns]
matrix = []
for x in centered:
    matrix.append([round(sum(a * b for a, b in zip(x, y)) / math.sqrt(sum(a * a for a in x) * sum(b * b for b in y)), 3) for y in centered])
await submit_answer({"matrix": matrix})
