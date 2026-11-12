import json
points = json.loads(await get_metrics({"host": "web-07", "metric": "mem"}))["points"]
values = [value for t, value in points if value is not None]
low, high = min(values), max(values)
await submit_answer({"normalized": [round((value - low) / (high - low), 3) for value in values], "min": low, "max": high})
