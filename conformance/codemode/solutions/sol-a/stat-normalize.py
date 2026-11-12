import json
points = json.loads(await get_metrics({"host": "web-07", "metric": "mem"}))["points"]
values = [v for t, v in points if v is not None]
lo, hi = min(values), max(values)
await submit_answer({"normalized": [round((v - lo) / (hi - lo), 3) for v in values], "min": lo, "max": hi})
