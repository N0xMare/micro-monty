import json
import statistics
points = json.loads(await get_metrics({"host": "db-01", "metric": "mem"}))["points"]
values = [v for t, v in points]
averages = []
for i in range(len(values) - 3):
    window = values[i:i + 4]
    averages.append(None if None in window else round(statistics.mean(window), 2))
await submit_answer({"averages": averages})
