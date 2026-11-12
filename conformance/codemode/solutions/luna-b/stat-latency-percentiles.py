import json
import math

data = json.loads(await get_metrics({"host": "web-03", "metric": "latency_ms"}))
values = sorted(v for _, v in data["points"] if v is not None)
def nearest(p): return values[math.ceil(p * len(values)) - 1]
await submit_answer({"p50": nearest(0.50), "p90": nearest(0.90), "p99": nearest(0.99)})
