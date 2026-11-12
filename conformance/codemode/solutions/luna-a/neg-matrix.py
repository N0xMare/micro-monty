import asyncio
import json
import math

metrics = ["cpu", "mem", "latency_ms"]
raw = await asyncio.gather(*(get_metrics({"host": "web-02", "metric": metric}) for metric in metrics))
series = [{timestamp: value for timestamp, value in json.loads(result)["points"] if value is not None} for result in raw]
common = set(series[0]) & set(series[1]) & set(series[2])
vectors = [[values[t] for t in sorted(common)] for values in series]

def corr(a, b):
    ma, mb = sum(a) / len(a), sum(b) / len(b)
    da, db = [x - ma for x in a], [y - mb for y in b]
    return sum(x * y for x, y in zip(da, db)) / math.sqrt(sum(x * x for x in da) * sum(y * y for y in db))

matrix = [[round(corr(a, b), 3) for b in vectors] for a in vectors]
await submit_answer({"matrix": matrix})
