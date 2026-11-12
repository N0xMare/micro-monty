import asyncio
import json

urls = [
    "https://api.example.com", "https://auth.example.com", "https://billing.example.com",
    "https://cdn.example.com", "https://mail.example.com", "https://search.example.com",
    "https://status.example.com",
]
responses = await asyncio.gather(*(check_health({"url": url}) for url in urls))
results = [json.loads(response) for response in responses]
down = sorted(result["url"] for result in results if not result["up"])
up_latencies = [result["latency_ms"] for result in results if result["up"] and result["latency_ms"] is not None]
average = round(sum(up_latencies) / len(up_latencies), 1) if up_latencies else 0.0
await submit_answer({"down": down, "avg_latency_ms": average})
