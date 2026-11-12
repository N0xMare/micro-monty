import json
import math

points = json.loads(await get_metrics({"host": "web-05", "metric": "cpu"}))["points"]
values = [float(p[1]) for p in points if p[1] is not None]
mean = sum(values) / len(values)
std = math.sqrt(sum((x - mean) ** 2 for x in values) / len(values))
timestamps = [p[0] for p in points if p[1] is not None and std > 0 and abs(float(p[1]) - mean) > 2 * std]
await submit_answer({"timestamps": timestamps})
