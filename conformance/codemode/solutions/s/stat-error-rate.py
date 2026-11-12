data = await get_metrics({"host": "web-06", "metric": "errors"})
values = [v for _, v in data["points"] if v is not None]
n = len(values)

per_hour = sum(values) / n * 4
zero_fraction = sum(1 for v in values if v == 0) / n

print(len(data["points"]), n, per_hour, zero_fraction)
await submit_answer({"per_hour": round(per_hour, 2), "zero_fraction": round(zero_fraction, 3)})
