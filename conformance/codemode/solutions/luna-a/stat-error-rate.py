import json

data = json.loads(await get_metrics({"host": "web-06", "metric": "errors"}))
values = [p[1] for p in data["points"] if p[1] is not None]
per_hour = sum(values) / len(values) * 4 if values else 0.0
zero_fraction = sum(v == 0 for v in values) / len(values) if values else 0.0
await submit_answer({"per_hour": round(per_hour, 2), "zero_fraction": round(zero_fraction, 3)})
