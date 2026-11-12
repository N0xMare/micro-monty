import asyncio

urls = [
    "https://api.example.com", "https://auth.example.com", "https://billing.example.com",
    "https://cdn.example.com", "https://mail.example.com", "https://search.example.com",
    "https://status.example.com",
]
results = await asyncio.gather(*(check_health({"url": url}) for url in urls))
down = sorted(result["url"] for result in results if not result["up"])
latencies = [result["latency_ms"] for result in results if result["up"] and result["latency_ms"] is not None]
avg = round(sum(latencies) / len(latencies), 1) if latencies else 0.0
await submit_answer({"down": down, "avg_latency_ms": avg})
