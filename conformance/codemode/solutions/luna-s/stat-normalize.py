result = await get_metrics({"host": "web-07", "metric": "mem"})
values = [value for _, value in result["points"] if value is not None]
low, high = min(values), max(values)
normalized = [round((value - low) / (high - low), 3) if high != low else 0.0 for value in values]
await submit_answer({"normalized": normalized, "min": low, "max": high})
