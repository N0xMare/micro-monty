import json
import asyncio

urls = [
    "https://api.example.com", "https://auth.example.com", "https://billing.example.com",
    "https://cdn.example.com", "https://mail.example.com", "https://search.example.com",
    "https://status.example.com",
]
results = await asyncio.gather(*(check_health({"url": u}) for u in urls), return_exceptions=True)
down = []
latencies = []
for u, r in zip(urls, results):
    if isinstance(r, Exception):
        print("probe failed", u, r)
        down.append(u)
        continue
    d = json.loads(r)
    if d["up"]:
        if d.get("latency_ms") is not None:
            latencies.append(d["latency_ms"])
    else:
        down.append(u)
avg = round(sum(latencies) / len(latencies), 1) if latencies else 0.0
print(sorted(down), avg)
await submit_answer({"down": sorted(down), "avg_latency_ms": avg})
