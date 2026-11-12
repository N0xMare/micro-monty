import json
import statistics

data = json.loads(await get_metrics({"host": "web-05", "metric": "cpu"}))
points = [(t, v) for t, v in data["points"] if v is not None]
values = [v for _, v in points]
mean = statistics.mean(values); sd = statistics.pstdev(values)
timestamps = [t for t, v in points if abs(v - mean) > 2 * sd]
await submit_answer({"timestamps": timestamps})
