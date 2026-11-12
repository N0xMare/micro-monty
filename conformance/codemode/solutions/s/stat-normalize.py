data = await get_metrics({"host": "web-07", "metric": "mem"})
vals = [v for _, v in sorted(data["points"], key=lambda p: p[0]) if v is not None]

lo, hi = min(vals), max(vals)
span = hi - lo
normalized = [round((v - lo) / span, 3) if span else 0.0 for v in vals]

print(len(vals), lo, hi, normalized)
await submit_answer({"normalized": normalized, "min": lo, "max": hi})
