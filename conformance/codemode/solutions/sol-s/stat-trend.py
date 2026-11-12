from statistics import linear_regression

series = await get_metrics({"host": "db-02", "metric": "latency_ms"})
pairs = [(i, value) for i, (timestamp, value) in enumerate(series["points"]) if value is not None]
x, y = zip(*pairs)
slope, intercept = linear_regression(x, y)
await submit_answer({"slope": round(slope, 4), "intercept": round(intercept, 2), "predicted_48": round(slope * 48 + intercept, 2)})
