import asyncio
import math

cpu, latency = await asyncio.gather(
    get_metrics({"host": "web-01", "metric": "cpu"}),
    get_metrics({"host": "web-01", "metric": "latency_ms"}),
)
ys = {t: v for t, v in cpu["points"] if v is not None}
paired = [(ys[t], v) for t, v in latency["points"] if v is not None and t in ys]
n = len(paired)
mx = sum(x for x, _ in paired) / n
my = sum(y for _, y in paired) / n
numerator = sum((x - mx) * (y - my) for x, y in paired)
denom = math.sqrt(sum((x - mx) ** 2 for x, _ in paired) * sum((y - my) ** 2 for _, y in paired))
await submit_answer({"r": round(numerator / denom, 3) if denom else 0.0, "n": n})
