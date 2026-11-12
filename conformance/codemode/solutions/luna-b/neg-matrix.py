import asyncio
import json
import math

metrics = ["cpu", "mem", "latency_ms"]
responses = await asyncio.gather(*(get_metrics({"host": "web-02", "metric": metric}) for metric in metrics))
series = [json.loads(response)["points"] for response in responses]
aligned = [{points[i][0]: points[i][1] for i in range(len(points))} for points in series]
timestamps = set(aligned[0]).intersection(*(set(values) for values in aligned[1:]))
timestamps = sorted(ts for ts in timestamps if all(values[ts] is not None for values in aligned))
vectors = [[values[ts] for ts in timestamps] for values in aligned]

def correlation(a, b):
    n = len(a)
    if n < 2:
        return 0.0
    mean_a, mean_b = sum(a) / n, sum(b) / n
    numerator = sum((x - mean_a) * (y - mean_b) for x, y in zip(a, b))
    denominator = math.sqrt(sum((x - mean_a) ** 2 for x in a) * sum((y - mean_b) ** 2 for y in b))
    return numerator / denominator if denominator else 0.0

matrix = [[round(correlation(a, b), 3) for b in vectors] for a in vectors]
await submit_answer({"matrix": matrix})
