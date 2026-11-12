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
down = sorted(u for u, r in zip(urls, results) if not r["up"])
latencies = [r["latency_ms"] for r in results if r["up"] and r["latency_ms"] is not None]
avg = round(sum(latencies) / len(latencies), 1) if latencies else 0.0
print(results)
print(down, avg)
await submit_answer({"down": down, "avg_latency_ms": avg})
