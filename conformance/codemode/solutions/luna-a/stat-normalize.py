import json

points = json.loads(await get_metrics({"host": "web-07", "metric": "mem"}))["points"]
values = [float(p[1]) for p in points if p[1] is not None]
lo, hi = min(values), max(values)
normalized = [round((v - lo) / (hi - lo), 3) if hi != lo else 0.0 for v in values]
await submit_answer({"normalized": normalized, "min": lo, "max": hi})
