import asyncio
import json
import statistics
raw = await asyncio.gather(*(get_metrics({"host": "web-01", "metric": metric}) for metric in ["cpu", "latency_ms"]))
cpu, latency = [{t: v for t, v in json.loads(r)["points"] if v is not None} for r in raw]
times = sorted(cpu.keys() & latency.keys())
r = statistics.correlation([cpu[t] for t in times], [latency[t] for t in times])
await submit_answer({"r": round(r, 3), "n": len(times)})
