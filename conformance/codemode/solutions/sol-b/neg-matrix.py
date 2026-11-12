import asyncio
import json
import math

metrics = ("cpu", "mem", "latency_ms")
raw = await asyncio.gather(*(get_metrics({"host": "web-02", "metric": metric}) for metric in metrics))
series = [{ts: value for ts, value in json.loads(result)["points"] if value is not None} for result in raw]
times = sorted(set.intersection(*(set(values) for values in series)))
columns = [[values[ts] for ts in times] for values in series]
centered = [[value - sum(column) / len(column) for value in column] for column in columns]
matrix = []
for x in centered:
    matrix.append([round(sum(a * b for a, b in zip(x, y)) / math.sqrt(sum(a * a for a in x) * sum(b * b for b in y)), 3) for y in centered])
await submit_answer({"matrix": matrix})
