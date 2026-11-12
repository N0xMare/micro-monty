import json
import math

points = json.loads(await get_metrics({"host": "web-03", "metric": "latency_ms"}))["points"]
values = sorted(p[1] for p in points if p[1] is not None)
def nearest(p):
    return values[max(0, math.ceil(p * len(values)) - 1)] if values else 0
await submit_answer({"p50": nearest(.50), "p90": nearest(.90), "p99": nearest(.99)})
