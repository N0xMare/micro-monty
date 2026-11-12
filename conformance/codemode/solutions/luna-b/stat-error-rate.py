import json

data = json.loads(await get_metrics({"host": "web-06", "metric": "errors"}))
values = [v for _, v in data["points"] if v is not None]
await submit_answer({"per_hour": round(4 * sum(values) / len(values), 2), "zero_fraction": round(sum(v == 0 for v in values) / len(values), 3)})
