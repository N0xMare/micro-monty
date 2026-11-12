import json
import statistics
points = json.loads(await get_metrics({"host": "web-05", "metric": "cpu"}))["points"]
values = [v for t, v in points if v is not None]
mean, deviation = statistics.mean(values), statistics.pstdev(values)
await submit_answer({"timestamps": sorted(t for t, v in points if v is not None and abs(v - mean) > 2 * deviation)})
