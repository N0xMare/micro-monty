import asyncio
import json
from math import sqrt, fsum

results = await asyncio.gather(*(get_metrics({"host": "web-02", "metric": metric}) for metric in ["cpu", "mem", "latency_ms"]))
series = [{ts: value for ts, value in json.loads(result)["points"] if value is not None} for result in results]
timestamps = sorted(set.intersection(*(set(values) for values in series)))
columns = [[values[ts] for ts in timestamps] for values in series]
centered = [[value - fsum(values) / len(values) for value in values] for values in columns]
matrix = [[round(fsum(x * y for x, y in zip(a, b)) / sqrt(fsum(x * x for x in a) * fsum(y * y for y in b)), 3) for b in centered] for a in centered]
await submit_answer({"matrix": matrix})
