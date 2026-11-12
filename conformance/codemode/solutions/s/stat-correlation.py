import asyncio
import statistics

cpu, lat = await asyncio.gather(
    get_metrics({"host": "web-01", "metric": "cpu"}),
    get_metrics({"host": "web-01", "metric": "latency_ms"}),
)

cpu_by_ts = {ts: v for ts, v in cpu["points"] if v is not None}
lat_by_ts = {ts: v for ts, v in lat["points"] if v is not None}
common = sorted(cpu_by_ts.keys() & lat_by_ts.keys())

xs = [cpu_by_ts[t] for t in common]
ys = [lat_by_ts[t] for t in common]
r = statistics.correlation(xs, ys)

print(len(cpu["points"]), len(lat["points"]), len(common), r)
await submit_answer({"r": round(r, 3), "n": len(common)})
