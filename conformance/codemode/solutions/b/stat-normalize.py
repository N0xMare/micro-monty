import json

data = json.loads(await get_metrics({"host": "web-07", "metric": "mem"}))
values = [v for _, v in sorted(data["points"], key=lambda p: p[0]) if v is not None]
lo, hi = min(values), max(values)
span = hi - lo
normalized = [round((v - lo) / span, 3) if span else 0.0 for v in values]

print(len(values), "samples, min", lo, "max", hi)
print(await submit_answer({"normalized": normalized, "min": lo, "max": hi}))
