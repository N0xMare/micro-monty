from statistics import mean

series = await get_metrics({"host": "web-06", "metric": "errors"})
values = [v for t, v in series["points"] if v is not None]
await submit_answer({"per_hour": round(mean(values) * 4, 2), "zero_fraction": round(sum(v == 0 for v in values) / len(values), 3)})
