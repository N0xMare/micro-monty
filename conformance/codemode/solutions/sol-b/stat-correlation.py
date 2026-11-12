import asyncio
import json
import statistics
raw_cpu, raw_latency = await asyncio.gather(get_metrics({"host": "web-01", "metric": "cpu"}), get_metrics({"host": "web-01", "metric": "latency_ms"}))
cpu = dict(json.loads(raw_cpu)["points"])
latency = dict(json.loads(raw_latency)["points"])
pairs = [(value, latency[t]) for t, value in cpu.items() if value is not None and latency.get(t) is not None]
x, y = zip(*pairs)
await submit_answer({"r": round(statistics.correlation(x, y), 3), "n": len(pairs)})
