import asyncio
import json

urls = [
    "https://api.example.com", "https://auth.example.com", "https://billing.example.com",
    "https://cdn.example.com", "https://mail.example.com", "https://search.example.com",
    "https://status.example.com",
]
results = await asyncio.gather(*(check_health({"url": url}) for url in urls))
health = [json.loads(result) for result in results]
up = [item["latency_ms"] for item in health if item["up"] and item["latency_ms"] is not None]
down = sorted(item["url"] for item in health if not item["up"])
average = round(sum(up) / len(up), 1) if up else 0.0
await submit_answer({"down": down, "avg_latency_ms": average})
