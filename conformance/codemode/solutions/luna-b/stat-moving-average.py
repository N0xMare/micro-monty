import json

data = json.loads(await get_metrics({"host": "db-01", "metric": "mem"}))
values = [v for _, v in data["points"]]
averages = [round(sum(w) / 4, 2) if all(v is not None for v in (w := values[i:i + 4])) else None for i in range(len(values) - 3)]
await submit_answer({"averages": averages})
