import json, statistics

data = json.loads(await get_metrics({"host": "web-05", "metric": "cpu"}))
pts = [(t, v) for t, v in data["points"] if v is not None]
vals = [v for _, v in pts]
mean = statistics.fmean(vals)
sd = statistics.pstdev(vals)
anomalies = sorted(t for t, v in pts if abs(v - mean) > 2 * sd)

await submit_answer({"timestamps": anomalies})
