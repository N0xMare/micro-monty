import json
import math
import asyncio

cpu_raw, lat_raw = await asyncio.gather(get_metrics({"host": "web-01", "metric": "cpu"}), get_metrics({"host": "web-01", "metric": "latency_ms"}))
cpu, lat = json.loads(cpu_raw)["points"], json.loads(lat_raw)["points"]
paired = [(float(a[1]), float(b[1])) for a, b in zip(cpu, lat) if a[1] is not None and b[1] is not None and a[0] == b[0]]
n = len(paired)
mx = sum(x for x, _ in paired) / n
my = sum(y for _, y in paired) / n
num = sum((x - mx) * (y - my) for x, y in paired)
den = math.sqrt(sum((x - mx) ** 2 for x, _ in paired) * sum((y - my) ** 2 for _, y in paired))
await submit_answer({"r": round(num / den, 3) if den else 0.0, "n": n})
