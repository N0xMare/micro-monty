import json
import statistics
points = json.loads(await get_metrics({"host": "web-06", "metric": "errors"}))["points"]
values = [value for t, value in points if value is not None]
await submit_answer({"per_hour": round(4 * statistics.mean(values), 2), "zero_fraction": round(values.count(0) / len(values), 3)})
