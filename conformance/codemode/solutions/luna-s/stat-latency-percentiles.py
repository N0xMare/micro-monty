import math

result = await get_metrics({"host": "web-03", "metric": "latency_ms"})
values = sorted(v for _, v in result["points"] if v is not None)
def nearest_rank(p):
    return values[math.ceil(p * len(values)) - 1]
await submit_answer({"p50": nearest_rank(0.50), "p90": nearest_rank(0.90), "p99": nearest_rank(0.99)})
