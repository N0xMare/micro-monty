import asyncio
import json

urls = [f"https://{name}.example.com" for name in ("api", "auth", "billing", "cdn", "mail", "search", "status")]
raw = await asyncio.gather(*(check_health({"url": url}) for url in urls))
results = [json.loads(result) for result in raw]
latencies = [result["latency_ms"] for result in results if result["up"]]
await submit_answer({"down": sorted(result["url"] for result in results if not result["up"]), "avg_latency_ms": round(sum(latencies) / len(latencies), 1)})
