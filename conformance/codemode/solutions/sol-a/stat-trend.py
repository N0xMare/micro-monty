import json
import statistics
points = json.loads(await get_metrics({"host": "db-02", "metric": "latency_ms"}))["points"]
pairs = [(i, value) for i, (timestamp, value) in enumerate(points) if value is not None]
slope, intercept = statistics.linear_regression([x for x, y in pairs], [y for x, y in pairs])
await submit_answer({"slope": round(slope, 4), "intercept": round(intercept, 2), "predicted_48": round(slope * 48 + intercept, 2)})
