import asyncio
import json
import statistics

cpu_raw, lat_raw = await asyncio.gather(
    get_metrics({"host": "web-01", "metric": "cpu"}),
    get_metrics({"host": "web-01", "metric": "latency_ms"}),
)
cpu = {t: v for t, v in json.loads(cpu_raw)["points"] if v is not None}
lat = {t: v for t, v in json.loads(lat_raw)["points"] if v is not None}
common = sorted(cpu.keys() & lat.keys())
xs = [cpu[t] for t in common]
ys = [lat[t] for t in common]

r = statistics.correlation(xs, ys)
answer = {"r": round(r, 3), "n": len(common)}
print(answer)
print(await submit_answer(answer))
