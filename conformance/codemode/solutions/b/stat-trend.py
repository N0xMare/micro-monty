import json, statistics

data = json.loads(await get_metrics({"host": "db-02", "metric": "latency_ms"}))
xs, ys = [], []
for i, (_, v) in enumerate(data["points"]):
    if v is not None:
        xs.append(i)
        ys.append(float(v))
slope, intercept = statistics.linear_regression(xs, ys)
result = {
    "slope": round(slope, 4),
    "intercept": round(intercept, 2),
    "predicted_48": round(slope * 48 + intercept, 2),
}
print(len(xs), result)
print(await submit_answer(result))
