import json
import math

data = json.loads(await get_metrics({"host": "web-03", "metric": "latency_ms"}))
values = sorted(v for _, v in data["points"] if v is not None)
n = len(values)

def nearest_rank(p):
    return values[max(1, math.ceil(p / 100 * n)) - 1]

answer = {"p50": nearest_rank(50), "p90": nearest_rank(90), "p99": nearest_rank(99)}
print(n, "samples", answer)
print(await submit_answer(answer))
