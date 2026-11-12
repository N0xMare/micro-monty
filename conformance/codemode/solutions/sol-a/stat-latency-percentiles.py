import json
points = json.loads(await get_metrics({"host": "web-03", "metric": "latency_ms"}))["points"]
values = sorted(v for t, v in points if v is not None)
await submit_answer({f"p{p}": values[(p * len(values) + 99) // 100 - 1] for p in [50, 90, 99]})
