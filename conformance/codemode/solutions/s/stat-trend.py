data = await get_metrics({"host": "db-02", "metric": "latency_ms"})
points = sorted(data["points"], key=lambda p: p[0])
xs, ys = [], []
for i, (_, v) in enumerate(points):
    if v is not None:
        xs.append(i)
        ys.append(float(v))

n = len(xs)
mx = sum(xs) / n
my = sum(ys) / n
sxx = sum((x - mx) ** 2 for x in xs)
sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
slope = sxy / sxx
intercept = my - slope * mx
pred = intercept + slope * 48
print(n, slope, intercept, pred)
await submit_answer({
    "slope": round(slope, 4),
    "intercept": round(intercept, 2),
    "predicted_48": round(pred, 2),
})
