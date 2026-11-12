import json
import statistics
points = json.loads(await get_metrics({"host": "web-06", "metric": "errors"}))["points"]
values = [v for t, v in points if v is not None]
await submit_answer({"per_hour": round(4 * statistics.mean(values), 2), "zero_fraction": round(sum(v == 0 for v in values) / len(values), 3)})
