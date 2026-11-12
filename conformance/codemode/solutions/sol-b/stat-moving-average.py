import json
points = json.loads(await get_metrics({"host": "db-01", "metric": "mem"}))["points"]
values = [value for t, value in points]
averages = []
for i in range(len(values) - 3):
    window = values[i:i + 4]
    averages.append(None if None in window else round(sum(window) / 4, 2))
await submit_answer({"averages": averages})
