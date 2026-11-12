import asyncio, json

urls = [
    "https://api.example.com",
    "https://auth.example.com",
    "https://billing.example.com",
    "https://cdn.example.com",
    "https://mail.example.com",
    "https://search.example.com",
    "https://status.example.com",
]
results = await asyncio.gather(*(check_health({"url": u}) for u in urls))
checks = [json.loads(r) for r in results]
down = sorted(u for u, c in zip(urls, checks) if not c["up"])
lat = [c["latency_ms"] for c in checks if c["up"] and c["latency_ms"] is not None]
avg = round(sum(lat) / len(lat), 1) if lat else 0.0
print(checks)
print(await submit_answer({"down": down, "avg_latency_ms": avg}))
