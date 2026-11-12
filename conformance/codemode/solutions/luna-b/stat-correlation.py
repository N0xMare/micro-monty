import asyncio
import json
import math

cpu_raw, lat_raw = await asyncio.gather(get_metrics({"host": "web-01", "metric": "cpu"}), get_metrics({"host": "web-01", "metric": "latency_ms"}))
cpu = {t: v for t, v in json.loads(cpu_raw)["points"] if v is not None}
lat = {t: v for t, v in json.loads(lat_raw)["points"] if v is not None}
pairs = [(cpu[t], lat[t]) for t in cpu.keys() & lat.keys()]
n = len(pairs); xs = [p[0] for p in pairs]; ys = [p[1] for p in pairs]
mx, my = sum(xs) / n, sum(ys) / n
r = sum((x - mx) * (y - my) for x, y in pairs) / math.sqrt(sum((x - mx) ** 2 for x in xs) * sum((y - my) ** 2 for y in ys))
await submit_answer({"r": round(r, 3), "n": n})
