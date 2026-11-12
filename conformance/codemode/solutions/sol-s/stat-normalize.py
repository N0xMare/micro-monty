series = await get_metrics({"host": "web-07", "metric": "mem"})
values = [v for t, v in sorted(series["points"]) if v is not None]
lo, hi = min(values), max(values)
normalized = [round((v - lo) / (hi - lo), 3) for v in values]
await submit_answer({"normalized": normalized, "min": lo, "max": hi})
