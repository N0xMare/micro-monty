import json

points = json.loads(await get_metrics({"host": "db-02", "metric": "latency_ms"}))["points"]
samples = [(i, float(p[1])) for i, p in enumerate(points) if p[1] is not None]
n = len(samples)
mx = sum(x for x, _ in samples) / n
my = sum(y for _, y in samples) / n
den = sum((x - mx) ** 2 for x, _ in samples)
slope = sum((x - mx) * (y - my) for x, y in samples) / den if den else 0.0
intercept = my - slope * mx
await submit_answer({"slope": round(slope, 4), "intercept": round(intercept, 2), "predicted_48": round(intercept + slope * 48, 2)})
