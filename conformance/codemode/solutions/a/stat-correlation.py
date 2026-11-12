import asyncio
import json
import math

cpu_raw, lat_raw = await asyncio.gather(
    get_metrics({"host": "web-01", "metric": "cpu"}),
    get_metrics({"host": "web-01", "metric": "latency_ms"}),
)
cpu = {t: v for t, v in json.loads(cpu_raw)["points"] if v is not None}
lat = {t: v for t, v in json.loads(lat_raw)["points"] if v is not None}

common = sorted(cpu.keys() & lat.keys())
xs = [cpu[t] for t in common]
ys = [lat[t] for t in common]
n = len(common)

mx, my = sum(xs) / n, sum(ys) / n
sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
sxx = sum((x - mx) ** 2 for x in xs)
syy = sum((y - my) ** 2 for y in ys)
r = sxy / math.sqrt(sxx * syy)

answer = {"r": round(r, 3), "n": n}
print(answer)
print(await submit_answer(answer))
