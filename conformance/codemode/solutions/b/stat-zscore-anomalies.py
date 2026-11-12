import json, statistics

data = json.loads(await get_metrics({"host": "web-05", "metric": "cpu"}))
points = [(t, float(v)) for t, v in data["points"] if v is not None]
values = [v for _, v in points]
mean = statistics.fmean(values)
sd = statistics.pstdev(values)
anomalies = sorted(t for t, v in points if abs(v - mean) > 2 * sd)
print(f"n={len(values)} mean={mean:.3f} pstdev={sd:.3f}", anomalies)
print(await submit_answer({"timestamps": anomalies}))
