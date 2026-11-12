import json

data = json.loads(await get_metrics({"host": "web-06", "metric": "errors"}))
values = [v for _, v in data["points"] if v is not None]

per_hour = sum(values) / len(values) * 4
zero_fraction = sum(1 for v in values if v == 0) / len(values)

answer = {"per_hour": round(per_hour, 2), "zero_fraction": round(zero_fraction, 3)}
print(len(data["points"]), "points,", len(values), "non-null", answer)
print(await submit_answer(answer))
