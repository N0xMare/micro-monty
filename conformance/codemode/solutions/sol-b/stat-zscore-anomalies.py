import json
import statistics
points = json.loads(await get_metrics({"host": "web-05", "metric": "cpu"}))["points"]
values = [value for t, value in points if value is not None]
mean = statistics.mean(values)
sd = statistics.pstdev(values)
await submit_answer({"timestamps": sorted(t for t, value in points if value is not None and abs(value - mean) > 2 * sd)})
