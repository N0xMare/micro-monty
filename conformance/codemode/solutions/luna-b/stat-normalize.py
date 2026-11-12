import json

data = json.loads(await get_metrics({"host": "web-07", "metric": "mem"}))
values = [v for _, v in data["points"] if v is not None]
low, high = min(values), max(values)
normalized = [round((v - low) / (high - low), 3) if high != low else 0.0 for v in values]
await submit_answer({"normalized": normalized, "min": low, "max": high})
