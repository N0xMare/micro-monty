import json

points = json.loads(await get_metrics({"host": "db-01", "metric": "mem"}))["points"]
values = [p[1] for p in points]
averages = []
for start in range(max(0, len(values) - 3)):
    window = values[start:start + 4]
    averages.append(round(sum(window) / 4, 2) if all(v is not None for v in window) else None)
await submit_answer({"averages": averages})
