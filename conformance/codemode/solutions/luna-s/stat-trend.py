import asyncio

result = await get_metrics({"host": "db-02", "metric": "latency_ms"})
points = [(index, value) for index, (_, value) in enumerate(result["points"]) if value is not None]
n = len(points)
mx = sum(x for x, _ in points) / n
my = sum(y for _, y in points) / n
slope = sum((x - mx) * (y - my) for x, y in points) / sum((x - mx) ** 2 for x, _ in points)
intercept = my - slope * mx
await submit_answer({"slope": round(slope, 4), "intercept": round(intercept, 2), "predicted_48": round(intercept + slope * 48, 2)})
