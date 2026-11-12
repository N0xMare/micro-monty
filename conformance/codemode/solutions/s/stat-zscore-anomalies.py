import statistics

data = await get_metrics({"host": "web-05", "metric": "cpu"})
points = sorted(data["points"], key=lambda p: p[0])
vals = [float(v) for _, v in points if v is not None]
mean = statistics.fmean(vals)
sd = statistics.pstdev(vals)
anomalies = [ts for ts, v in points if v is not None and sd > 0 and abs(float(v) - mean) > 2 * sd]
print(mean, sd, anomalies)
await submit_answer({"timestamps": anomalies})
