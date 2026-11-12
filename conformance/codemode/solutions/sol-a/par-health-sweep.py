import asyncio
import json
from statistics import mean

urls = [f"https://{name}.example.com" for name in ["api", "auth", "billing", "cdn", "mail", "search", "status"]]
results = await asyncio.gather(*(check_health({"url": url}) for url in urls))
health = [json.loads(result) for result in results]
down = sorted(item["url"] for item in health if not item["up"])
latencies = [item["latency_ms"] for item in health if item["up"] and item["latency_ms"] is not None]
await submit_answer({"down": down, "avg_latency_ms": round(mean(latencies), 1)})
