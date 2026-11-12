import json

data = json.loads(await get_metrics({"host": "db-02", "metric": "latency_ms"}))
pts = [(i, v) for i, (_, v) in enumerate(data["points"]) if v is not None]
n = len(pts)
mx = sum(x for x, _ in pts) / n
my = sum(y for _, y in pts) / n
sxy = sum((x - mx) * (y - my) for x, y in pts)
sxx = sum((x - mx) ** 2 for x, _ in pts)
slope = sxy / sxx
intercept = my - slope * mx

await submit_answer({
    "slope": round(slope, 4),
    "intercept": round(intercept, 2),
    "predicted_48": round(intercept + slope * 48, 2),
})
