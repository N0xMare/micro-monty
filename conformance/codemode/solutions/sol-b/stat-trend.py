import json
import statistics
points = json.loads(await get_metrics({"host": "db-02", "metric": "latency_ms"}))["points"]
pairs = [(i, value) for i, (t, value) in enumerate(points) if value is not None]
x, y = zip(*pairs)
slope, intercept = statistics.linear_regression(x, y)
await submit_answer({"slope": round(slope, 4), "intercept": round(intercept, 2), "predicted_48": round(intercept + 48 * slope, 2)})
