import asyncio
from statistics import correlation

cpu, latency = await asyncio.gather(get_metrics({"host": "web-01", "metric": "cpu"}), get_metrics({"host": "web-01", "metric": "latency_ms"}))
cpu_by_time = {t: v for t, v in cpu["points"] if v is not None}
pairs = [(cpu_by_time[t], v) for t, v in latency["points"] if v is not None and t in cpu_by_time]
x, y = zip(*pairs)
await submit_answer({"r": round(correlation(x, y), 3), "n": len(pairs)})
